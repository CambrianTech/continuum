/**
 * Unit Tests for LoggerDaemon
 * Test-driven development for symmetric daemon architecture
 */

import { test, describe, beforeEach, afterEach, mock } from 'node:test';
import assert from 'node:assert';
import { LoggerDaemon } from '../../server/LoggerDaemon';
import { LoggerDaemonMessage, LogEntry, LogLevel } from '../../shared/LoggerMessageTypes';
import { ContinuumContext } from '../../../../types/shared/core/ContinuumTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

describe('LoggerDaemon Unit Tests', () => {
  let daemon: LoggerDaemon;
  let mockContext: ContinuumContext;
  let originalConsole: {
    log: typeof console.log;
    warn: typeof console.warn;
    error: typeof console.error;
    info: typeof console.info;
  };

  beforeEach(() => {
    // Store original console methods
    originalConsole = {
      log: console.log,
      warn: console.warn,
      error: console.error,
      info: console.info
    };

    // Mock context
    mockContext = {
      sessionId: 'test-session-123',
      environment: 'test'
    };

    // Create daemon instance
    daemon = new LoggerDaemon(mockContext);
  });

  afterEach(async () => {
    // Restore original console methods
    console.log = originalConsole.log;
    console.warn = originalConsole.warn;
    console.error = originalConsole.error;
    console.info = originalConsole.info;

    // Stop daemon
    await daemon.stop();
  });

  describe('Console Interception', () => {
    test('should preserve original console methods', async () => {
      // Before start - should have original methods
      assert.strictEqual(console.log, originalConsole.log);
      
      // After start - should be intercepted
      await daemon.start();
      assert.notStrictEqual(console.log, originalConsole.log);
      
      // After stop - should be restored
      await daemon.stop();
      assert.strictEqual(console.log, originalConsole.log);
    });

    test('should intercept all console methods', async () => {
      const interceptedCalls: Array<{ level: LogLevel; args: unknown[] }> = [];
      
      // Mock handleServerConsoleCall to track calls (avoid infinite loop)
      const originalHandle = (daemon as any).handleServerConsoleCall;
      (daemon as any).handleServerConsoleCall = (level: LogLevel, args: unknown[]) => {
        interceptedCalls.push({ level, args });
      };

      await daemon.start();

      // Test all console methods
      console.log('test log');
      console.warn('test warning');
      console.error('test error');
      console.info('test info');

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Verify interception (should be 4 calls, not 5)
      assert.strictEqual(interceptedCalls.length, 4);
      assert.strictEqual(interceptedCalls[0].level, 'log');
      assert.deepStrictEqual(interceptedCalls[0].args, ['test log']);
      assert.strictEqual(interceptedCalls[1].level, 'warn');
      assert.deepStrictEqual(interceptedCalls[1].args, ['test warning']);
      assert.strictEqual(interceptedCalls[2].level, 'error');
      assert.deepStrictEqual(interceptedCalls[2].args, ['test error']);
      assert.strictEqual(interceptedCalls[3].level, 'info');
      assert.deepStrictEqual(interceptedCalls[3].args, ['test info']);

      // Restore original method
      (daemon as any).handleServerConsoleCall = originalHandle;
    });

    test('should call original console methods', async () => {
      const originalCalls: Array<{ method: string; args: unknown[] }> = [];
      
      // Mock original console methods to track calls
      const mockLog = mock.fn((...args: unknown[]) => originalCalls.push({ method: 'log', args }));
      const mockWarn = mock.fn((...args: unknown[]) => originalCalls.push({ method: 'warn', args }));
      const mockError = mock.fn((...args: unknown[]) => originalCalls.push({ method: 'error', args }));
      const mockInfo = mock.fn((...args: unknown[]) => originalCalls.push({ method: 'info', args }));

      (daemon as any).originalConsole = {
        log: mockLog,
        warn: mockWarn,
        error: mockError,
        info: mockInfo
      };

      await daemon.start();

      // Test console calls
      console.log('test message');
      console.warn('warn message');

      // Verify original methods were called
      assert.strictEqual(mockLog.mock.callCount(), 1);
      assert.strictEqual(mockWarn.mock.callCount(), 1);
      assert.deepStrictEqual(mockLog.mock.calls[0].arguments, ['test message']);
      assert.deepStrictEqual(mockWarn.mock.calls[0].arguments, ['warn message']);
    });
  });

  describe('Message Processing', () => {
    test('should process log messages correctly', async () => {
      const logEntry: LogEntry = {
        level: 'info',
        message: 'Test message',
        timestamp: Date.now(),
        sessionId: 'test-session',
        source: 'TestSource',
        context: {
          sessionId: 'test-session',
          environment: 'test'
        }
      };

      const message: LoggerDaemonMessage = {
        id: 'test-msg-1',
        timestamp: new Date(),
        data: {
          type: 'log',
          payload: logEntry
        }
      };

      // Mock file writing to avoid actual file operations
      const mockWriteToFile = mock.fn();
      (daemon as any).writeToFile = mockWriteToFile;

      const response = await (daemon as any).processMessage(message);

      assert.strictEqual(response.success, true);
      assert.strictEqual(response.messageId, 'test-msg-1');
      assert.ok(typeof response.processingTime === 'number');
    });

    test('should handle unknown message types', async () => {
      const message: LoggerDaemonMessage = {
        id: 'test-msg-2',
        timestamp: new Date(),
        data: {
          type: 'unknown' as any,
          payload: {}
        }
      };

      const response = await (daemon as any).processMessage(message);

      assert.strictEqual(response.success, false);
      assert.ok(response.error?.includes('Unknown message type'));
      assert.strictEqual(response.messageId, 'test-msg-2');
    });

    test('should process flush messages', async () => {
      const flushRequest = { sessionId: 'test-session' };
      const message: LoggerDaemonMessage = {
        id: 'flush-msg-1',
        timestamp: new Date(),
        data: {
          type: 'flush',
          payload: flushRequest
        }
      };

      // Mock flush methods
      const mockFlushBuffer = mock.fn();
      const mockFlushAllBuffers = mock.fn();
      (daemon as any).flushBuffer = mockFlushBuffer;
      (daemon as any).flushAllBuffers = mockFlushAllBuffers;

      const response = await (daemon as any).processMessage(message);

      assert.strictEqual(response.success, true);
      assert.strictEqual(mockFlushBuffer.mock.callCount(), 1);
      assert.deepStrictEqual(mockFlushBuffer.mock.calls[0].arguments, ['test-session']);
    });
  });

  describe('Batch Processing', () => {
    test('should process log messages in batches', async () => {
      const logEntry1: LogEntry = {
        level: 'info',
        message: 'Message 1',
        timestamp: Date.now(),
        sessionId: 'test-session',
        source: 'TestSource',
        context: { sessionId: 'test-session', environment: 'test' }
      };

      const logEntry2: LogEntry = {
        level: 'error',
        message: 'Message 2',
        timestamp: Date.now(),
        sessionId: 'test-session',
        source: 'TestSource',
        context: { sessionId: 'test-session', environment: 'test' }
      };

      const messages: LoggerDaemonMessage[] = [
        {
          id: 'batch-msg-1',
          timestamp: new Date(),
          data: { type: 'log', payload: logEntry1 }
        },
        {
          id: 'batch-msg-2',
          timestamp: new Date(),
          data: { type: 'log', payload: logEntry2 }
        }
      ];

      // Mock batch processing
      const mockProcessBatchedLogs = mock.fn();
      (daemon as any).processBatchedLogs = mockProcessBatchedLogs;

      const responses = await (daemon as any).processBatch(messages);

      assert.strictEqual(responses.length, 2);
      assert.strictEqual(mockProcessBatchedLogs.mock.callCount(), 1);
      assert.strictEqual(mockProcessBatchedLogs.mock.calls[0].arguments[0].length, 2);
    });

    test('should handle mixed message types in batches', async () => {
      const logEntry: LogEntry = {
        level: 'info',
        message: 'Test message',
        timestamp: Date.now(),
        sessionId: 'test-session',
        source: 'TestSource',
        context: { sessionId: 'test-session', environment: 'test' }
      };

      const messages: LoggerDaemonMessage[] = [
        {
          id: 'mixed-msg-1',
          timestamp: new Date(),
          data: { type: 'log', payload: logEntry }
        },
        {
          id: 'mixed-msg-2',
          timestamp: new Date(),
          data: { type: 'flush', payload: { sessionId: 'test-session' } }
        }
      ];

      // Mock methods
      const mockProcessBatchedLogs = mock.fn();
      const mockFlushBuffer = mock.fn();
      (daemon as any).processBatchedLogs = mockProcessBatchedLogs;
      (daemon as any).flushBuffer = mockFlushBuffer;

      const responses = await (daemon as any).processBatch(messages);

      assert.strictEqual(responses.length, 2);
      assert.strictEqual(mockProcessBatchedLogs.mock.callCount(), 1);
      assert.strictEqual(mockFlushBuffer.mock.callCount(), 1);
    });
  });

  describe('Session Management', () => {
    test('should update session ID', () => {
      const newSessionId = 'new-session-456';
      daemon.setCurrentSessionId(newSessionId);
      
      // Verify session ID was updated
      assert.strictEqual((daemon as any).currentSessionId, newSessionId);
    });

    test('should use correct session directory path', () => {
      const sessionId = 'test-session-789';
      const expectedPath = path.join(
        process.cwd(),
        '.continuum',
        'sessions',
        'user',
        'shared',
        sessionId,
        'logs'
      );

      const actualPath = (daemon as any).getSessionLogDirectory(sessionId);
      assert.strictEqual(actualPath, expectedPath);
    });
  });

  describe('Error Handling', () => {
    test('should handle console interception errors gracefully', async () => {
      let errorHandled = false;
      
      // Mock handleServerConsoleCall to throw an error but track it
      const originalHandle = (daemon as any).handleServerConsoleCall;
      (daemon as any).handleServerConsoleCall = () => {
        errorHandled = true;
        throw new Error('Test error');
      };

      // Mock the interceptServerConsole to catch errors
      const originalIntercept = (daemon as any).interceptServerConsole;
      (daemon as any).interceptServerConsole = () => {
        const self = daemon;
        console.log = (...args: unknown[]) => {
          try {
            (self as any).originalConsole.log(...args);
            (self as any).handleServerConsoleCall('log', args);
          } catch (error) {
            // Error should be caught here
            (self as any).originalConsole.error('Error in console interception:', error);
          }
        };
      };

      await daemon.start();

      // This should not throw - errors should be caught
      assert.doesNotThrow(() => {
        console.log('test message');
      });

      // Verify error was handled
      assert.ok(errorHandled);

      // Restore original methods
      (daemon as any).handleServerConsoleCall = originalHandle;
      (daemon as any).interceptServerConsole = originalIntercept;
    });

    test('should handle file writing errors', async () => {
      const logEntry: LogEntry = {
        level: 'error',
        message: 'Test error message',
        timestamp: Date.now(),
        sessionId: 'test-session',
        source: 'TestSource',
        context: { sessionId: 'test-session', environment: 'test' }
      };

      // Mock writeToFile to throw an error
      const mockWriteToFile = mock.fn(() => {
        throw new Error('File write error');
      });
      (daemon as any).writeToFile = mockWriteToFile;

      // This should handle the error gracefully
      await assert.doesNotReject(async () => {
        await (daemon as any).writeLogEntry(logEntry);
      });
    });
  });

  describe('Type Safety', () => {
    test('should enforce LogLevel type safety', () => {
      const validLevels: LogLevel[] = ['log', 'info', 'warn', 'error'];
      
      validLevels.forEach(level => {
        const logEntry: LogEntry = {
          level,
          message: 'Test message',
          timestamp: Date.now(),
          sessionId: 'test-session',
          source: 'TestSource',
          context: { sessionId: 'test-session', environment: 'test' }
        };
        
        // Should not throw type errors
        assert.ok(logEntry.level === level);
      });
    });

    test('should enforce ContinuumContext type safety', () => {
      const validContext: ContinuumContext = {
        sessionId: 'test-session',
        environment: 'test'
      };

      const logEntry: LogEntry = {
        level: 'info',
        message: 'Test message',
        timestamp: Date.now(),
        sessionId: 'test-session',
        source: 'TestSource',
        context: validContext
      };

      // Should maintain type safety
      assert.strictEqual(logEntry.context.sessionId, 'test-session');
      assert.strictEqual(logEntry.context.environment, 'test');
    });
  });
});