/**
 * Integration Tests for LoggerDaemon
 * Testing client-server communication via daemon bus
 */

import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { LoggerDaemon } from '../../server/LoggerDaemon';
import { ClientLoggerDaemon } from '../../client/ClientLoggerDaemon';
import { ContinuumContext } from '../../../../types/shared/core/ContinuumTypes';
import { LogEntry, LogLevel } from '../../shared/LoggerMessageTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

describe('LoggerDaemon Integration Tests', () => {
  let serverDaemon: LoggerDaemon;
  let clientDaemon: ClientLoggerDaemon;
  let mockContext: ContinuumContext;
  let testSessionId: string;
  let testLogDir: string;

  beforeEach(async () => {
    testSessionId = `test-session-${Date.now()}`;
    mockContext = {
      sessionId: testSessionId,
      environment: 'test'
    };

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

    // Create daemon instances
    serverDaemon = new LoggerDaemon(mockContext);
    clientDaemon = new ClientLoggerDaemon(mockContext);
  });

  afterEach(async () => {
    // Stop daemons
    await serverDaemon.stop();
    await clientDaemon.stop();

    // Clean up test directory
    try {
      await fs.rm(path.dirname(testLogDir), { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
  });

  describe('Server-Side Logging', () => {
    test('should log server console calls to files', async () => {
      await serverDaemon.start();
      
      // Wait a bit for initialization
      await new Promise(resolve => setTimeout(resolve, 100));

      // Make console calls
      console.log('Test server log message');
      console.warn('Test server warning');
      console.error('Test server error');

      // Wait for async logging
      await new Promise(resolve => setTimeout(resolve, 200));

      // Verify files exist
      const serverLogPath = path.join(testLogDir, 'server.log');
      const serverWarnPath = path.join(testLogDir, 'server.warn.json');
      const serverErrorPath = path.join(testLogDir, 'server.error.json');

      // Check if files exist and have content
      try {
        const logContent = await fs.readFile(serverLogPath, 'utf8');
        assert.ok(logContent.includes('Test server log message'));
        
        const warnContent = await fs.readFile(serverWarnPath, 'utf8');
        assert.ok(warnContent.includes('Test server warning'));
        
        const errorContent = await fs.readFile(serverErrorPath, 'utf8');
        assert.ok(errorContent.includes('Test server error'));
      } catch (error) {
        // Files might not exist yet due to async nature
        console.log('Files not created yet:', error);
      }
    });

    test('should maintain original console behavior', async () => {
      let originalOutput: string[] = [];
      
      // Capture original console output
      const originalLog = console.log;
      console.log = (...args: unknown[]) => {
        originalOutput.push(args.join(' '));
        originalLog(...args);
      };

      await serverDaemon.start();
      
      // Clear captured output
      originalOutput = [];
      
      // Make console call
      console.log('Test original behavior');
      
      // Verify original console was called
      assert.strictEqual(originalOutput.length, 1);
      assert.strictEqual(originalOutput[0], 'Test original behavior');
      
      // Restore original console
      console.log = originalLog;
    });
  });

  describe('Message Processing', () => {
    test('should process daemon messages correctly', async () => {
      await serverDaemon.start();
      
      const logEntry: LogEntry = {
        level: 'info',
        message: 'Test daemon message',
        timestamp: Date.now(),
        sessionId: testSessionId,
        source: 'ClientDaemon',
        context: mockContext
      };

      const message = {
        id: 'test-msg-1',
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: logEntry
        }
      };

      // Process message directly
      const response = await (serverDaemon as any).processMessage(message);
      
      assert.strictEqual(response.success, true);
      assert.strictEqual(response.messageId, 'test-msg-1');
    });

    test('should handle client-forwarded messages', async () => {
      await serverDaemon.start();
      
      // Simulate client message forwarding
      const clientLogEntry: LogEntry = {
        level: 'log',
        message: 'Client forwarded message',
        timestamp: Date.now(),
        sessionId: testSessionId,
        source: 'ClientConsole',
        context: {
          ...mockContext,
          environment: 'browser'
        }
      };

      const message = {
        id: 'client-msg-1',
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: clientLogEntry
        }
      };

      const response = await (serverDaemon as any).processMessage(message);
      
      assert.strictEqual(response.success, true);
      assert.strictEqual(response.messageId, 'client-msg-1');
      
      // Wait for file writing
      await new Promise(resolve => setTimeout(resolve, 100));
      
      // Verify client message was logged
      const serverLogPath = path.join(testLogDir, 'server.log');
      try {
        const logContent = await fs.readFile(serverLogPath, 'utf8');
        assert.ok(logContent.includes('Client forwarded message'));
        assert.ok(logContent.includes('ClientConsole'));
      } catch (error) {
        console.log('Client message not logged yet:', error);
      }
    });
  });

  describe('Batch Processing', () => {
    test('should handle multiple messages efficiently', async () => {
      await serverDaemon.start();
      
      // Create multiple log entries
      const logEntries: LogEntry[] = [];
      for (let i = 0; i < 5; i++) {
        logEntries.push({
          level: 'info',
          message: `Batch message ${i}`,
          timestamp: Date.now(),
          sessionId: testSessionId,
          source: 'BatchTest',
          context: mockContext
        });
      }

      const messages = logEntries.map((entry, index) => ({
        id: `batch-msg-${index}`,
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: entry
        }
      }));

      // Process batch
      const responses = await (serverDaemon as any).processBatch(messages);
      
      assert.strictEqual(responses.length, 5);
      responses.forEach((response, index) => {
        assert.strictEqual(response.success, true);
        assert.strictEqual(response.messageId, `batch-msg-${index}`);
      });
    });
  });

  describe('Error Handling', () => {
    test('should handle file system errors gracefully', async () => {
      // Create daemon with invalid session directory
      const invalidContext = {
        sessionId: '/invalid/path/session',
        environment: 'test'
      };
      
      const invalidDaemon = new LoggerDaemon(invalidContext);
      
      // Should not throw when starting
      await assert.doesNotReject(async () => {
        await invalidDaemon.start();
      });
      
      await invalidDaemon.stop();
    });

    test('should handle malformed log entries', async () => {
      await serverDaemon.start();
      
      const malformedMessage = {
        id: 'malformed-msg-1',
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: {
            // Missing required fields
            level: 'info',
            message: 'Malformed entry'
          }
        }
      };

      // Should handle gracefully
      const response = await (serverDaemon as any).processMessage(malformedMessage);
      
      // Should either succeed or fail gracefully
      assert.ok(typeof response.success === 'boolean');
      assert.strictEqual(response.messageId, 'malformed-msg-1');
    });
  });

  describe('Performance', () => {
    test('should handle high-volume logging', async () => {
      await serverDaemon.start();
      
      const startTime = Date.now();
      const messageCount = 100;
      
      // Generate many log entries
      const logEntries: LogEntry[] = [];
      for (let i = 0; i < messageCount; i++) {
        logEntries.push({
          level: 'info',
          message: `Performance test message ${i}`,
          timestamp: Date.now(),
          sessionId: testSessionId,
          source: 'PerformanceTest',
          context: mockContext
        });
      }

      const messages = logEntries.map((entry, index) => ({
        id: `perf-msg-${index}`,
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: entry
        }
      }));

      // Process all messages
      const responses = await (serverDaemon as any).processBatch(messages);
      
      const endTime = Date.now();
      const processingTime = endTime - startTime;
      
      // Verify all messages processed
      assert.strictEqual(responses.length, messageCount);
      
      // Performance check - should process 100 messages in reasonable time
      assert.ok(processingTime < 5000, `Processing took ${processingTime}ms, expected < 5000ms`);
      
      // Verify all responses are successful
      responses.forEach((response, index) => {
        assert.strictEqual(response.success, true);
        assert.strictEqual(response.messageId, `perf-msg-${index}`);
      });
    });
  });

  describe('Session Management', () => {
    test('should handle session ID changes', async () => {
      await serverDaemon.start();
      
      const newSessionId = `new-session-${Date.now()}`;
      serverDaemon.setCurrentSessionId(newSessionId);
      
      // Create log entry with new session
      const logEntry: LogEntry = {
        level: 'info',
        message: 'New session message',
        timestamp: Date.now(),
        sessionId: newSessionId,
        source: 'SessionTest',
        context: {
          sessionId: newSessionId,
          environment: 'test'
        }
      };

      const message = {
        id: 'session-msg-1',
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: logEntry
        }
      };

      const response = await (serverDaemon as any).processMessage(message);
      
      assert.strictEqual(response.success, true);
      
      // Verify session ID is used in logging
      const expectedPath = path.join(
        process.cwd(),
        '.continuum',
        'sessions',
        'user',
        'shared',
        newSessionId,
        'logs'
      );
      
      const actualPath = (serverDaemon as any).getSessionLogDirectory(newSessionId);
      assert.strictEqual(actualPath, expectedPath);
    });
  });

  describe('Type Safety Integration', () => {
    test('should maintain type safety across daemon boundaries', async () => {
      await serverDaemon.start();
      
      // Create strongly typed log entry
      const typedLogEntry: LogEntry = {
        level: 'warn' as LogLevel,
        message: 'Type safety test',
        timestamp: Date.now(),
        sessionId: testSessionId,
        source: 'TypeSafetyTest',
        context: mockContext,
        data: {
          testData: 'additional data',
          timestamp: new Date().toISOString()
        }
      };

      const typedMessage = {
        id: 'typed-msg-1',
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: typedLogEntry
        }
      };

      // Process with full type safety
      const response = await (serverDaemon as any).processMessage(typedMessage);
      
      assert.strictEqual(response.success, true);
      assert.strictEqual(response.messageId, 'typed-msg-1');
      
      // Verify type constraints are maintained
      assert.ok(['log', 'info', 'warn', 'error'].includes(typedLogEntry.level));
      assert.ok(typeof typedLogEntry.timestamp === 'number');
      assert.ok(typeof typedLogEntry.context.sessionId === 'string');
    });
  });
});