/**
 * Simple Console Override Semaphore Test
 * Just test the critical run-once behavior
 */

import { test } from 'node:test';
import assert from 'node:assert';
import { ConsoleOverrideSemaphore } from '../ConsoleOverrideSemaphore';

test('Console Override Semaphore - Basic Run-Once Test', () => {
  // Reset state
  ConsoleOverrideSemaphore._forceReset();

  // First acquire should work
  assert.doesNotThrow(() => {
    ConsoleOverrideSemaphore.acquire('TestSource');
  });

  assert.strictEqual(ConsoleOverrideSemaphore.isActive(), true);
  assert.strictEqual(ConsoleOverrideSemaphore.getCurrentSource(), 'TestSource');

  // Second acquire should CRASH
  assert.throws(() => {
    ConsoleOverrideSemaphore.acquire('SecondSource');
  }, /CONSOLE_OVERRIDE_SEMAPHORE_VIOLATION/);

  // First source should still be active
  assert.strictEqual(ConsoleOverrideSemaphore.isActive(), true);
  assert.strictEqual(ConsoleOverrideSemaphore.getCurrentSource(), 'TestSource');

  // Release should work
  assert.doesNotThrow(() => {
    ConsoleOverrideSemaphore.release('TestSource');
  });

  assert.strictEqual(ConsoleOverrideSemaphore.isActive(), false);
  assert.strictEqual(ConsoleOverrideSemaphore.getCurrentSource(), null);

  // After release, new acquire should work
  assert.doesNotThrow(() => {
    ConsoleOverrideSemaphore.acquire('NewSource');
  });

  assert.strictEqual(ConsoleOverrideSemaphore.isActive(), true);
  assert.strictEqual(ConsoleOverrideSemaphore.getCurrentSource(), 'NewSource');

  // Clean up
  ConsoleOverrideSemaphore._forceReset();
});

test('Console Override Semaphore - Wrong Source Release Test', () => {
  // Reset state
  ConsoleOverrideSemaphore._forceReset();

  // Acquire with one source
  ConsoleOverrideSemaphore.acquire('CorrectSource');

  // Try to release with wrong source - should throw
  assert.throws(() => {
    ConsoleOverrideSemaphore.release('WrongSource');
  }, /CONSOLE_OVERRIDE_SEMAPHORE_VIOLATION/);

  // Original source should still be active
  assert.strictEqual(ConsoleOverrideSemaphore.isActive(), true);
  assert.strictEqual(ConsoleOverrideSemaphore.getCurrentSource(), 'CorrectSource');

  // Clean up
  ConsoleOverrideSemaphore._forceReset();
});

test('Console Override Semaphore - Console Method Storage Test', () => {
  // Reset state
  ConsoleOverrideSemaphore._forceReset();

  // Store original methods
  const originalLog = console.log;
  const originalInfo = console.info;
  const originalWarn = console.warn;
  const originalError = console.error;
  const originalDebug = console.debug;

  // Acquire semaphore
  ConsoleOverrideSemaphore.acquire('TestSource');

  // Get stored original methods
  const storedMethods = ConsoleOverrideSemaphore.getOriginalConsole();
  assert.notStrictEqual(storedMethods, null);
  assert.strictEqual(storedMethods?.log, originalLog);
  assert.strictEqual(storedMethods?.info, originalInfo);
  assert.strictEqual(storedMethods?.warn, originalWarn);
  assert.strictEqual(storedMethods?.error, originalError);
  assert.strictEqual(storedMethods?.debug, originalDebug);

  // Override console methods
  const mockLog = () => {};
  console.log = mockLog;

  // Release should restore original methods
  ConsoleOverrideSemaphore.release('TestSource');

  // Console methods should be restored
  assert.strictEqual(console.log, originalLog);
  assert.strictEqual(console.info, originalInfo);
  assert.strictEqual(console.warn, originalWarn);
  assert.strictEqual(console.error, originalError);
  assert.strictEqual(console.debug, originalDebug);
});

console.log('✅ Console Override Semaphore tests completed');