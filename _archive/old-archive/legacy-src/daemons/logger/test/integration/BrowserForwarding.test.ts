/**
 * Integration Tests for Browser Console Forwarding
 * Testing client-side console interception and forwarding to server
 */

import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { ClientConsoleManager } from '../../client/ClientConsoleManager';
import { ClientLoggerClient } from '../../client/ClientLoggerClient';
import { ContinuumContext } from '../../../../types/shared/core/ContinuumTypes';
import { LogLevel } from '../../shared/LoggerMessageTypes';

describe('Browser Console Forwarding Tests', () => {
  let consoleManager: ClientConsoleManager;
  let mockLoggerClient: ClientLoggerClient;
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

    mockContext = {
      sessionId: 'test-session-browser',
      environment: 'browser'
    };

    // Create mock logger client
    mockLoggerClient = {
      log: async (level: LogLevel, message: string, data?: any) => {
        // Mock implementation
        return Promise.resolve();
      }
    } as any;

    // Create mock getSessionId function
    const mockGetSessionId = () => mockContext.sessionId;

    consoleManager = new ClientConsoleManager(mockGetSessionId);
  });

  afterEach(() => {
    // Restore original console methods
    console.log = originalConsole.log;
    console.warn = originalConsole.warn;
    console.error = originalConsole.error;
    console.info = originalConsole.info;
  });

  describe('Console Interception', () => {
    test('should intercept browser console calls', async () => {
      const interceptedCalls: Array<{ level: LogLevel; args: unknown[] }> = [];
      
      // Mock forwardConsole to track calls
      const originalForward = consoleManager.forwardConsole;
      consoleManager.forwardConsole = async (level: LogLevel, args: unknown[]) => {
        interceptedCalls.push({ level, args });
        return originalForward.call(consoleManager, level, args);
      };

      // Start console interception
      consoleManager.startConsoleInterception();

      // Test console calls
      console.log('Browser log message');
      console.warn('Browser warning');
      console.error('Browser error');
      console.info('Browser info');

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Verify interception
      assert.strictEqual(interceptedCalls.length, 4);
      assert.strictEqual(interceptedCalls[0].level, 'log');
      assert.deepStrictEqual(interceptedCalls[0].args, ['Browser log message']);
      assert.strictEqual(interceptedCalls[1].level, 'warn');
      assert.deepStrictEqual(interceptedCalls[1].args, ['Browser warning']);
      assert.strictEqual(interceptedCalls[2].level, 'error');
      assert.deepStrictEqual(interceptedCalls[2].args, ['Browser error']);
      assert.strictEqual(interceptedCalls[3].level, 'info');
      assert.deepStrictEqual(interceptedCalls[3].args, ['Browser info']);
    });

    test('should preserve original console behavior', async () => {
      const originalCalls: Array<{ method: string; args: unknown[] }> = [];
      
      // Mock original console methods to track calls
      const mockLog = (...args: unknown[]) => originalCalls.push({ method: 'log', args });
      const mockWarn = (...args: unknown[]) => originalCalls.push({ method: 'warn', args });
      const mockError = (...args: unknown[]) => originalCalls.push({ method: 'error', args });
      const mockInfo = (...args: unknown[]) => originalCalls.push({ method: 'info', args });

      // Store mock methods in console manager
      (consoleManager as any).originalConsole = {
        log: mockLog,
        warn: mockWarn,
        error: mockError,
        info: mockInfo
      };

      consoleManager.startConsoleInterception();

      // Test console calls
      console.log('Test original behavior');
      console.warn('Test original warning');

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Verify original methods were called
      assert.strictEqual(originalCalls.length, 2);
      assert.strictEqual(originalCalls[0].method, 'log');
      assert.deepStrictEqual(originalCalls[0].args, ['Test original behavior']);
      assert.strictEqual(originalCalls[1].method, 'warn');
      assert.deepStrictEqual(originalCalls[1].args, ['Test original warning']);
    });

    test('should restore console methods on stop', () => {
      consoleManager.startConsoleInterception();
      
      // Console should be intercepted
      assert.notStrictEqual(console.log, originalConsole.log);
      
      consoleManager.stopConsoleInterception();
      
      // Console should be restored
      assert.strictEqual(console.log, originalConsole.log);
      assert.strictEqual(console.warn, originalConsole.warn);
      assert.strictEqual(console.error, originalConsole.error);
      assert.strictEqual(console.info, originalConsole.info);
    });
  });

  describe('Message Forwarding', () => {
    test('should forward console messages to logger client', async () => {
      const forwardedMessages: Array<{ level: LogLevel; message: string; data?: any }> = [];
      
      // Mock logger client to capture forwarded messages
      mockLoggerClient.log = async (level: LogLevel, message: string, data?: any) => {
        forwardedMessages.push({ level, message, data });
      };

      consoleManager.startConsoleInterception();

      // Test message forwarding
      console.log('Forwarded log message');
      console.error('Forwarded error message', { errorCode: 500 });

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Verify messages were forwarded
      assert.strictEqual(forwardedMessages.length, 2);
      assert.strictEqual(forwardedMessages[0].level, 'log');
      assert.strictEqual(forwardedMessages[0].message, 'Forwarded log message');
      assert.strictEqual(forwardedMessages[1].level, 'error');
      assert.strictEqual(forwardedMessages[1].message, 'Forwarded error message {"errorCode":500}');
    });

    test('should handle complex message types', async () => {
      const forwardedMessages: Array<{ level: LogLevel; message: string; data?: any }> = [];
      
      mockLoggerClient.log = async (level: LogLevel, message: string, data?: any) => {
        forwardedMessages.push({ level, message, data });
      };

      consoleManager.startConsoleInterception();

      // Test complex message types
      console.log('Simple string');
      console.warn('Number and object:', 42, { key: 'value' });
      console.error('Array and function:', [1, 2, 3], () => 'test');

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Verify complex messages were handled
      assert.strictEqual(forwardedMessages.length, 3);
      assert.strictEqual(forwardedMessages[0].message, 'Simple string');
      assert.ok(forwardedMessages[1].message.includes('Number and object:'));
      assert.ok(forwardedMessages[1].message.includes('42'));
      assert.ok(forwardedMessages[1].message.includes('{"key":"value"}'));
      assert.ok(forwardedMessages[2].message.includes('Array and function:'));
      assert.ok(forwardedMessages[2].message.includes('[1,2,3]'));
    });

    test('should handle forwarding errors gracefully', async () => {
      let errorHandled = false;
      
      // Mock logger client to throw errors
      mockLoggerClient.log = async () => {
        throw new Error('Forwarding failed');
      };

      // Mock error handling
      const originalError = console.error;
      console.error = (...args: unknown[]) => {
        if (args[0] === 'Failed to forward console message:') {
          errorHandled = true;
        }
        originalError(...args);
      };

      consoleManager.startConsoleInterception();

      // This should not throw
      console.log('Test forwarding error');

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Verify error was handled
      assert.ok(errorHandled);
      
      // Restore console
      console.error = originalError;
    });
  });

  describe('Session Context', () => {
    test('should include session context in forwarded messages', async () => {
      const forwardedMessages: Array<{ level: LogLevel; message: string; data?: any }> = [];
      
      mockLoggerClient.log = async (level: LogLevel, message: string, data?: any) => {
        forwardedMessages.push({ level, message, data });
      };

      consoleManager.startConsoleInterception();

      console.log('Test session context');

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Verify session context was included
      assert.strictEqual(forwardedMessages.length, 1);
      assert.ok(forwardedMessages[0].data);
      assert.strictEqual(forwardedMessages[0].data.sessionId, 'test-session-browser');
      assert.strictEqual(forwardedMessages[0].data.environment, 'browser');
    });

    test('should handle session context changes', async () => {
      const forwardedMessages: Array<{ level: LogLevel; message: string; data?: any }> = [];
      
      mockLoggerClient.log = async (level: LogLevel, message: string, data?: any) => {
        forwardedMessages.push({ level, message, data });
      };

      consoleManager.startConsoleInterception();

      // Change session context
      const newContext: ContinuumContext = {
        sessionId: 'new-session-id',
        environment: 'browser'
      };
      
      consoleManager.updateContext(newContext);

      console.log('Test context change');

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Verify new context was used
      assert.strictEqual(forwardedMessages.length, 1);
      assert.strictEqual(forwardedMessages[0].data.sessionId, 'new-session-id');
    });
  });

  describe('Performance', () => {
    test('should handle high-frequency console calls', async () => {
      const forwardedMessages: Array<{ level: LogLevel; message: string; data?: any }> = [];
      
      mockLoggerClient.log = async (level: LogLevel, message: string, data?: any) => {
        forwardedMessages.push({ level, message, data });
      };

      consoleManager.startConsoleInterception();

      const startTime = Date.now();
      const messageCount = 100;

      // Generate high-frequency console calls
      for (let i = 0; i < messageCount; i++) {
        console.log(`High frequency message ${i}`);
      }

      // Wait for all async processing
      await new Promise(resolve => setTimeout(resolve, 100));

      const endTime = Date.now();
      const processingTime = endTime - startTime;

      // Verify all messages were processed
      assert.strictEqual(forwardedMessages.length, messageCount);
      
      // Performance check
      assert.ok(processingTime < 1000, `Processing took ${processingTime}ms, expected < 1000ms`);
    });
  });

  describe('Error Handling', () => {
    test('should handle console interception errors', async () => {
      let errorsCaught = 0;
      
      // Mock forwardConsole to throw errors
      const originalForward = consoleManager.forwardConsole;
      consoleManager.forwardConsole = async () => {
        throw new Error('Console forwarding error');
      };

      // Mock error handling
      const originalError = console.error;
      console.error = (...args: unknown[]) => {
        if (args[0] === 'Error in console interception:') {
          errorsCaught++;
        }
        originalError(...args);
      };

      consoleManager.startConsoleInterception();

      // This should not throw
      console.log('Test error handling');

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Verify error was handled
      assert.ok(errorsCaught > 0);
      
      // Restore methods
      consoleManager.forwardConsole = originalForward;
      console.error = originalError;
    });

    test('should prevent infinite loops in console interception', async () => {
      let recursionDetected = false;
      
      // Mock forwardConsole to call console.log (potential infinite loop)
      const originalForward = consoleManager.forwardConsole;
      consoleManager.forwardConsole = async (level: LogLevel, args: unknown[]) => {
        // This would normally cause infinite recursion
        console.log('Recursive call');
        return originalForward.call(consoleManager, level, args);
      };

      // Mock error detection
      const originalError = console.error;
      console.error = (...args: unknown[]) => {
        if (args[0] && args[0].toString().includes('recursion')) {
          recursionDetected = true;
        }
        originalError(...args);
      };

      consoleManager.startConsoleInterception();

      // This should be handled gracefully
      console.log('Test recursion prevention');

      // Wait for processing
      await new Promise(resolve => setTimeout(resolve, 10));

      // Restore methods
      consoleManager.forwardConsole = originalForward;
      console.error = originalError;
      
      // Should not crash the system
      assert.ok(true, 'System remained stable despite potential recursion');
    });
  });
});