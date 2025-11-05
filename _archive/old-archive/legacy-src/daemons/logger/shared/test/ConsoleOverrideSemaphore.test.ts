/**
 * Console Override Semaphore Tests
 * Testing the run-once semaphore pattern with crash protection
 */

import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { ConsoleOverrideSemaphore } from '../ConsoleOverrideSemaphore';

describe('ConsoleOverrideSemaphore', () => {
  let originalConsole: any;

  beforeEach(() => {
    // Store original console methods
    originalConsole = {
      log: console.log,
      info: console.info,
      warn: console.warn,
      error: console.error,
      debug: console.debug
    };

    // Reset semaphore state
    ConsoleOverrideSemaphore._forceReset();
  });

  afterEach(() => {
    // Restore original console methods
    console.log = originalConsole.log;
    console.info = originalConsole.info;
    console.warn = originalConsole.warn;
    console.error = originalConsole.error;
    console.debug = originalConsole.debug;

    // Reset semaphore state
    ConsoleOverrideSemaphore._forceReset();
  });

  describe('Basic Semaphore Operations', () => {
    test('should allow first acquire', () => {
      assert.doesNotThrow(() => {
        ConsoleOverrideSemaphore.acquire('TestSource');
      });

      assert.strictEqual(ConsoleOverrideSemaphore.isActive(), true);
      assert.strictEqual(ConsoleOverrideSemaphore.getCurrentSource(), 'TestSource');
    });

    test('should store original console methods on acquire', () => {
      ConsoleOverrideSemaphore.acquire('TestSource');
      const originalMethods = ConsoleOverrideSemaphore.getOriginalConsole();
      
      assert.notStrictEqual(originalMethods, null);
      assert.strictEqual(originalMethods?.log, originalConsole.log);
      assert.strictEqual(originalMethods?.info, originalConsole.info);
      assert.strictEqual(originalMethods?.warn, originalConsole.warn);
      assert.strictEqual(originalMethods?.error, originalConsole.error);
      assert.strictEqual(originalMethods?.debug, originalConsole.debug);
    });

    test('should allow release by same source', () => {
      ConsoleOverrideSemaphore.acquire('TestSource');
      
      assert.doesNotThrow(() => {
        ConsoleOverrideSemaphore.release('TestSource');
      });

      assert.strictEqual(ConsoleOverrideSemaphore.isActive(), false);
      assert.strictEqual(ConsoleOverrideSemaphore.getCurrentSource(), null);
    });

    test('should restore console methods on release', () => {
      ConsoleOverrideSemaphore.acquire('TestSource');
      
      // Override console methods
      const mockLog = () => {};
      console.log = mockLog;
      
      ConsoleOverrideSemaphore.release('TestSource');
      
      // Original console methods should be restored
      assert.strictEqual(console.log, originalConsole.log);
      assert.strictEqual(console.info, originalConsole.info);
      assert.strictEqual(console.warn, originalConsole.warn);
      assert.strictEqual(console.error, originalConsole.error);
      assert.strictEqual(console.debug, originalConsole.debug);
    });
  });

  describe('CRITICAL: Run-Once Enforcement', () => {
    test('should CRASH on second acquire attempt', () => {
      ConsoleOverrideSemaphore.acquire('FirstSource');
      
      expect(() => {
        ConsoleOverrideSemaphore.acquire('SecondSource');
      }).toThrow('CONSOLE_OVERRIDE_SEMAPHORE_VIOLATION');
    });

    test('should CRASH with detailed error message', () => {
      ConsoleOverrideSemaphore.acquire('FirstSource');
      
      expect(() => {
        ConsoleOverrideSemaphore.acquire('SecondSource');
      }).toThrow(/Console override already active by 'FirstSource'/);
      
      expect(() => {
        ConsoleOverrideSemaphore.acquire('SecondSource');
      }).toThrow(/Cannot acquire from 'SecondSource'/);
      
      expect(() => {
        ConsoleOverrideSemaphore.acquire('SecondSource');
      }).toThrow(/CRASHING IMMEDIATELY to prevent system damage/);
    });

    test('should maintain first source ownership during crash', () => {
      ConsoleOverrideSemaphore.acquire('FirstSource');
      
      try {
        ConsoleOverrideSemaphore.acquire('SecondSource');
      } catch (error) {
        // Verify first source is still active
        expect(ConsoleOverrideSemaphore.isActive()).toBe(true);
        expect(ConsoleOverrideSemaphore.getCurrentSource()).toBe('FirstSource');
      }
    });
  });

  describe('Release Validation', () => {
    test('should reject release when not active', () => {
      expect(() => {
        ConsoleOverrideSemaphore.release('TestSource');
      }).toThrow('No console override active');
    });

    test('should reject release from wrong source', () => {
      ConsoleOverrideSemaphore.acquire('FirstSource');
      
      expect(() => {
        ConsoleOverrideSemaphore.release('WrongSource');
      }).toThrow(/Console override owned by 'FirstSource'/);
      
      expect(() => {
        ConsoleOverrideSemaphore.release('WrongSource');
      }).toThrow(/Cannot release from 'WrongSource'/);
    });

    test('should maintain state after failed release', () => {
      ConsoleOverrideSemaphore.acquire('FirstSource');
      
      try {
        ConsoleOverrideSemaphore.release('WrongSource');
      } catch (error) {
        // Should still be active with original source
        expect(ConsoleOverrideSemaphore.isActive()).toBe(true);
        expect(ConsoleOverrideSemaphore.getCurrentSource()).toBe('FirstSource');
      }
    });
  });

  describe('Lifecycle Management', () => {
    test('should allow reacquire after proper release', () => {
      // First cycle
      ConsoleOverrideSemaphore.acquire('FirstSource');
      ConsoleOverrideSemaphore.release('FirstSource');
      
      // Second cycle should work
      expect(() => {
        ConsoleOverrideSemaphore.acquire('SecondSource');
      }).not.toThrow();
      
      expect(ConsoleOverrideSemaphore.isActive()).toBe(true);
      expect(ConsoleOverrideSemaphore.getCurrentSource()).toBe('SecondSource');
    });

    test('should maintain original console methods across cycles', () => {
      // First cycle
      ConsoleOverrideSemaphore.acquire('FirstSource');
      const firstOriginal = ConsoleOverrideSemaphore.getOriginalConsole();
      ConsoleOverrideSemaphore.release('FirstSource');
      
      // Second cycle
      ConsoleOverrideSemaphore.acquire('SecondSource');
      const secondOriginal = ConsoleOverrideSemaphore.getOriginalConsole();
      
      // Should be same original methods
      expect(secondOriginal?.log).toBe(firstOriginal?.log);
      expect(secondOriginal?.info).toBe(firstOriginal?.info);
      expect(secondOriginal?.warn).toBe(firstOriginal?.warn);
      expect(secondOriginal?.error).toBe(firstOriginal?.error);
      expect(secondOriginal?.debug).toBe(firstOriginal?.debug);
    });
  });

  describe('Testing Infrastructure', () => {
    test('_forceReset should reset all state', () => {
      ConsoleOverrideSemaphore.acquire('TestSource');
      
      // Override console methods
      const mockLog = jest.fn();
      console.log = mockLog;
      
      ConsoleOverrideSemaphore._forceReset();
      
      // All state should be reset
      expect(ConsoleOverrideSemaphore.isActive()).toBe(false);
      expect(ConsoleOverrideSemaphore.getCurrentSource()).toBeNull();
      expect(ConsoleOverrideSemaphore.getOriginalConsole()).toBeNull();
      
      // Console methods should be restored
      expect(console.log).toBe(originalConsole.log);
    });

    test('_forceReset should allow new acquire', () => {
      ConsoleOverrideSemaphore.acquire('FirstSource');
      ConsoleOverrideSemaphore._forceReset();
      
      expect(() => {
        ConsoleOverrideSemaphore.acquire('SecondSource');
      }).not.toThrow();
      
      expect(ConsoleOverrideSemaphore.isActive()).toBe(true);
      expect(ConsoleOverrideSemaphore.getCurrentSource()).toBe('SecondSource');
    });
  });

  describe('Edge Cases', () => {
    test('should handle empty source string', () => {
      expect(() => {
        ConsoleOverrideSemaphore.acquire('');
      }).not.toThrow();
      
      expect(ConsoleOverrideSemaphore.getCurrentSource()).toBe('');
    });

    test('should handle special characters in source', () => {
      const specialSource = 'Test/Source-123_áéíóú';
      
      expect(() => {
        ConsoleOverrideSemaphore.acquire(specialSource);
      }).not.toThrow();
      
      expect(ConsoleOverrideSemaphore.getCurrentSource()).toBe(specialSource);
    });

    test('should handle multiple failed acquire attempts', () => {
      ConsoleOverrideSemaphore.acquire('FirstSource');
      
      // Multiple failed attempts should all throw
      expect(() => ConsoleOverrideSemaphore.acquire('Second')).toThrow();
      expect(() => ConsoleOverrideSemaphore.acquire('Third')).toThrow();
      expect(() => ConsoleOverrideSemaphore.acquire('Fourth')).toThrow();
      
      // Original source should still be active
      expect(ConsoleOverrideSemaphore.getCurrentSource()).toBe('FirstSource');
    });
  });
});