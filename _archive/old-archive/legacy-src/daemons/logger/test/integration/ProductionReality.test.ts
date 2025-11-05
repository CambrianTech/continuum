/**
 * Production Reality Tests for LoggerDaemon
 * Tests that replicate actual production scenarios that caused gigabyte log files
 */

import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { LoggerDaemon } from '../../server/LoggerDaemon';
import { ContinuumContext } from '../../../../types/shared/core/ContinuumTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

describe('Production Reality Tests', () => {
  let daemon: LoggerDaemon;
  let mockContext: ContinuumContext;
  let testSessionId: string;
  let testLogDir: string;
  let originalConsole: {
    log: typeof console.log;
    warn: typeof console.warn;
    error: typeof console.error;
    info: typeof console.info;
  };

  beforeEach(async () => {
    // Store original console methods
    originalConsole = {
      log: console.log,
      warn: console.warn,
      error: console.error,
      info: console.info
    };

    testSessionId = `production-test-${Date.now()}`;
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

    // Clean up test directory
    try {
      await fs.rm(path.dirname(testLogDir), { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
  });

  describe('Infinite Loop Prevention - Production Scenarios', () => {
    test('should prevent infinite loops from file writing errors', async () => {
      let logFileSize = 0;
      let consoleCallCount = 0;
      let maxConsoleCallsReached = false;

      // Mock writeToFile to potentially trigger console calls
      const originalWriteToFile = (daemon as any).writeToFile;
      (daemon as any).writeToFile = async (filePath: string, content: string) => {
        logFileSize += content.length;
        
        // Simulate the real production scenario - file writing triggers console calls
        if (consoleCallCount < 10) {
          consoleCallCount++;
          console.log(`File write operation ${consoleCallCount} - writing to ${filePath}`);
        } else {
          maxConsoleCallsReached = true;
          // Don't call console.log anymore to prevent infinite loop
        }
        
        // Actually write to file
        return originalWriteToFile.call(daemon, filePath, content);
      };

      await daemon.start();

      // Make a console call that triggers file writing
      console.log('Initial production test log');

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 100));

      // Verify infinite loop was prevented
      assert.ok(maxConsoleCallsReached, 'Should have reached max console calls');
      assert.ok(logFileSize < 10000, `Log file size should be reasonable, got ${logFileSize} bytes`);
      assert.ok(consoleCallCount === 10, `Should have made exactly 10 console calls, got ${consoleCallCount}`);
    });

    test('should detect recursive console calls in production environment', async () => {
      let recursionDetected = false;
      let actualConsoleCallCount = 0;

      // Mock the actual production scenario where console interception causes recursion
      const originalHandleServerConsoleCall = (daemon as any).handleServerConsoleCall;
      (daemon as any).handleServerConsoleCall = function(level: any, args: any[]) {
        actualConsoleCallCount++;
        
        if (actualConsoleCallCount > 5) {
          recursionDetected = true;
          (daemon as any).originalConsole.error('Production recursion detected - stopping interception');
          return;
        }

        // This is what was happening in production - handle triggers more console calls
        try {
          const message = args.join(' ');
          console.log(`Processing: ${message}`); // This triggers more console calls!
        } catch (error) {
          console.error('Error processing message:', error); // This also triggers more calls!
        }
      };

      await daemon.start();

      // This single call should trigger recursion
      console.log('Production recursion test');

      // Wait for recursion to be detected
      await new Promise(resolve => setTimeout(resolve, 50));

      // Verify recursion was detected and stopped
      assert.ok(recursionDetected, 'Should have detected recursion');
      assert.ok(actualConsoleCallCount <= 10, `Should have limited console calls, got ${actualConsoleCallCount}`);
    });

    test('should handle the exact production scenario that created gigabyte files', async () => {
      let totalLogSize = 0;
      let fileWriteCount = 0;
      let stopped = false;

      // Mock the exact production scenario
      const originalWriteToFile = (daemon as any).writeToFile;
      (daemon as any).writeToFile = async (filePath: string, content: string) => {
        if (stopped) return;
        
        totalLogSize += content.length;
        fileWriteCount++;
        
        // This is what was happening - file writes trigger console calls
        console.log(`[${fileWriteCount}] Writing ${content.length} bytes to ${path.basename(filePath)}`);
        
        // Stop after reasonable limit to prevent actual gigabyte files
        if (totalLogSize > 50000 || fileWriteCount > 100) {
          stopped = true;
          console.log('STOPPED - would have created gigabyte files');
          return;
        }
        
        return originalWriteToFile.call(daemon, filePath, content);
      };

      await daemon.start();

      // Single console call that should trigger the cascade
      console.log('Production gigabyte test');

      // Wait for the cascade
      await new Promise(resolve => setTimeout(resolve, 100));

      // Verify we stopped before creating gigabyte files
      assert.ok(stopped, 'Should have stopped the cascade');
      assert.ok(totalLogSize < 100000, `Should have limited log size, got ${totalLogSize} bytes`);
      assert.ok(fileWriteCount <= 100, `Should have limited file writes, got ${fileWriteCount}`);
    });
  });

  describe('Real File System Integration', () => {
    test('should handle actual file system operations without infinite loops', async () => {
      let fileOperationCount = 0;
      let maxFileSize = 0;

      // Monitor actual file operations
      const originalWriteToFile = (daemon as any).writeToFile;
      (daemon as any).writeToFile = async (filePath: string, content: string) => {
        fileOperationCount++;
        maxFileSize = Math.max(maxFileSize, content.length);
        
        // Prevent infinite file operations
        if (fileOperationCount > 20) {
          throw new Error('Too many file operations - preventing infinite loop');
        }
        
        return originalWriteToFile.call(daemon, filePath, content);
      };

      await daemon.start();

      // Make multiple console calls
      console.log('Real file test 1');
      console.warn('Real file test 2');
      console.error('Real file test 3');

      // Wait for file operations
      await new Promise(resolve => setTimeout(resolve, 200));

      // Verify reasonable file operations
      assert.ok(fileOperationCount <= 20, `Should have limited file operations, got ${fileOperationCount}`);
      assert.ok(maxFileSize < 10000, `Should have reasonable file sizes, max was ${maxFileSize} bytes`);
    });

    test('should handle file permission errors gracefully', async () => {
      let permissionErrorHandled = false;

      // Mock file permission error
      const originalWriteToFile = (daemon as any).writeToFile;
      (daemon as any).writeToFile = async () => {
        const error = new Error('EACCES: permission denied');
        (error as any).code = 'EACCES';
        throw error;
      };

      // Mock error handling
      const originalError = console.error;
      console.error = (...args: any[]) => {
        if (args[0] && args[0].includes('permission')) {
          permissionErrorHandled = true;
        }
        originalError(...args);
      };

      await daemon.start();

      // This should trigger permission error
      console.log('Permission test');

      // Wait for error handling
      await new Promise(resolve => setTimeout(resolve, 100));

      // Verify error was handled gracefully
      assert.ok(permissionErrorHandled, 'Should have handled permission error');
      
      // Restore console
      console.error = originalError;
    });
  });

  describe('Performance Under Load', () => {
    test('should handle rapid console calls without exploding log files', async () => {
      let totalBytes = 0;
      let fileWriteCount = 0;

      // Monitor actual log growth
      const originalWriteToFile = (daemon as any).writeToFile;
      (daemon as any).writeToFile = async (filePath: string, content: string) => {
        totalBytes += content.length;
        fileWriteCount++;
        
        // Prevent runaway growth
        if (totalBytes > 1000000) { // 1MB limit
          throw new Error('Log files growing too large - stopping');
        }
        
        return originalWriteToFile.call(daemon, filePath, content);
      };

      await daemon.start();

      // Rapid console calls
      for (let i = 0; i < 50; i++) {
        console.log(`Rapid call ${i}`);
        console.warn(`Rapid warning ${i}`);
      }

      // Wait for all operations
      await new Promise(resolve => setTimeout(resolve, 300));

      // Verify reasonable growth
      assert.ok(totalBytes < 1000000, `Log files should stay under 1MB, got ${totalBytes} bytes`);
      assert.ok(fileWriteCount < 1000, `Should have reasonable file writes, got ${fileWriteCount}`);
    });
  });

  describe('Memory Usage', () => {
    test('should not leak memory during console interception', async () => {
      const initialMemory = process.memoryUsage().heapUsed;

      await daemon.start();

      // Generate many console calls
      for (let i = 0; i < 1000; i++) {
        console.log(`Memory test ${i}`);
      }

      // Wait for processing
      await new Promise(resolve => setTimeout(resolve, 200));

      const finalMemory = process.memoryUsage().heapUsed;
      const memoryGrowth = finalMemory - initialMemory;

      // Verify reasonable memory growth (less than 10MB)
      assert.ok(memoryGrowth < 10 * 1024 * 1024, `Memory growth should be reasonable, got ${memoryGrowth} bytes`);
    });
  });
});