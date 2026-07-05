/**
 * Test ID Generator Utility
 * 
 * Provides unique test identifiers for test isolation and debugging.
 */

/**
 * Generate a unique test identifier with timestamp and random suffix
 */
export function createTestId(prefix: string = 'test'): string {
  const timestamp = new Date().toISOString().replace(/[^0-9]/g, '').slice(0, 14);
  const random = Math.random().toString(36).substring(2, 8);
  return `${prefix}-${timestamp}-${random}`;
}

/**
 * Generate a session-based test identifier
 */
export function createSessionTestId(sessionId: string, testName: string): string {
  const timestamp = Date.now().toString(36);
  return `${sessionId}-${testName}-${timestamp}`;
}

/**
 * Generate test-specific directory name
 */
export function createTestDir(testId: string): string {
  return `.continuum/tests/${testId}`;
}

/**
 * Generate test-specific log file path
 */
export function createTestLogPath(testId: string, logName: string): string {
  return `.continuum/tests/${testId}/logs/${logName}`;
}