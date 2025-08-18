/**
 * Command Testing Utilities - Reusable patterns for testing JTAG commands
 * 
 * These utilities provide consistent testing patterns that can be used by all commands.
 * Follows middle-out testing methodology with proper Layer 3 (Command System) validation.
 */

import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGClient } from '../../../system/core/client/shared/JTAGClient';

/**
 * Test environment info - matches what real commands return
 */
export interface TestEnvironmentInfo {
  type: 'browser' | 'server';
  timestamp: string;
  nodeVersion?: string;
  userAgent?: string;
  platform: string;
  [key: string]: any;
}

/**
 * Command test result validation
 */
export interface CommandTestResult {
  success: boolean;
  data?: any;
  error?: string;
  executionTime?: number;
  environment?: TestEnvironmentInfo;
}

/**
 * Create test context for command testing
 */
export function createTestContext(environment: 'browser' | 'server' = 'server', sessionId?: UUID): JTAGContext {
  // Import and use secure context creation
  const { createTestContext: createSecureTestContext, createClientContext } = require('../../system/core/context/SecureJTAGContext');
  const context = environment === 'browser' ? createClientContext() : createSecureTestContext();
  
  // Override UUID if provided (for test consistency)
  if (sessionId) {
    return { ...context, uuid: sessionId };
  }
  
  return context;
}

/**
 * Create test payload with required JTAG fields
 */
export function createTestPayload<T>(
  context: JTAGContext, 
  sessionId: UUID, 
  data: T
): T & JTAGPayload {
  return {
    context,
    sessionId,
    ...data
  };
}

/**
 * Validate command result structure
 */
export function validateCommandResult(result: any, expectedFields: string[] = []): boolean {
  if (!result || typeof result !== 'object') {
    return false;
  }

  // All command results should have success field
  if (typeof result.success !== 'boolean') {
    return false;
  }

  // Check for expected fields
  for (const field of expectedFields) {
    if (!(field in result)) {
      return false;
    }
  }

  return true;
}

/**
 * Validate environment info structure
 */
export function validateEnvironmentInfo(envInfo: any): envInfo is TestEnvironmentInfo {
  if (!envInfo || typeof envInfo !== 'object') {
    return false;
  }

  return (
    typeof envInfo.type === 'string' &&
    ['browser', 'server'].includes(envInfo.type) &&
    typeof envInfo.timestamp === 'string' &&
    typeof envInfo.platform === 'string'
  );
}

/**
 * Create mock command execution function for unit tests
 */
export function createMockCommandExecution<TParams extends JTAGPayload, TResult>(
  mockResult: TResult,
  executionTimeMs: number = 50
): (params: TParams) => Promise<TResult> {
  return async (params: TParams): Promise<TResult> => {
    // Simulate execution time
    await new Promise(resolve => setTimeout(resolve, executionTimeMs));
    
    // Validate params have required JTAG fields
    if (!params.context || !params.sessionId) {
      throw new Error('Invalid command params: missing context or sessionId');
    }
    
    return mockResult;
  };
}

/**
 * Test command execution with timeout
 */
export async function testCommandWithTimeout<TResult>(
  commandFn: () => Promise<TResult>,
  timeoutMs: number = 5000,
  commandName: string = 'unknown'
): Promise<TResult> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`Command '${commandName}' timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    commandFn()
      .then(result => {
        clearTimeout(timer);
        resolve(result);
      })
      .catch(error => {
        clearTimeout(timer);
        reject(error);
      });
  });
}

/**
 * Assert command result matches expected structure
 */
export function assertCommandResult(
  result: any, 
  expected: Partial<CommandTestResult>,
  testName: string = 'command test'
): void {
  if (expected.success !== undefined && result.success !== expected.success) {
    throw new Error(`${testName}: Expected success=${expected.success}, got ${result.success}`);
  }

  if (expected.error && (!result.error || !result.error.includes(expected.error))) {
    throw new Error(`${testName}: Expected error containing "${expected.error}", got "${result.error}"`);
  }

  if (expected.data && JSON.stringify(result.data) !== JSON.stringify(expected.data)) {
    throw new Error(`${testName}: Data mismatch. Expected: ${JSON.stringify(expected.data)}, got: ${JSON.stringify(result.data)}`);
  }

  if (expected.environment && !validateEnvironmentInfo(result.environment)) {
    throw new Error(`${testName}: Invalid environment info structure`);
  }
}

/**
 * Test error handling scenarios
 */
export async function testCommandErrorHandling<TParams extends JTAGPayload>(
  commandFn: (params: TParams) => Promise<any>,
  invalidParams: TParams[],
  expectedErrors: string[]
): Promise<void> {
  if (invalidParams.length !== expectedErrors.length) {
    throw new Error('invalidParams and expectedErrors arrays must have same length');
  }

  for (let i = 0; i < invalidParams.length; i++) {
    const params = invalidParams[i];
    const expectedError = expectedErrors[i];

    try {
      await commandFn(params);
      throw new Error(`Expected command to throw error containing "${expectedError}" for params: ${JSON.stringify(params)}`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      if (!errorMessage.includes(expectedError)) {
        throw new Error(`Expected error containing "${expectedError}", got: "${errorMessage}"`);
      }
    }
  }
}

/**
 * Performance testing utility
 */
export async function testCommandPerformance<TResult>(
  commandFn: () => Promise<TResult>,
  maxExecutionTimeMs: number,
  commandName: string = 'unknown'
): Promise<{ result: TResult; executionTime: number }> {
  const startTime = Date.now();
  const result = await commandFn();
  const executionTime = Date.now() - startTime;

  if (executionTime > maxExecutionTimeMs) {
    throw new Error(`Command '${commandName}' took ${executionTime}ms, expected < ${maxExecutionTimeMs}ms`);
  }

  return { result, executionTime };
}