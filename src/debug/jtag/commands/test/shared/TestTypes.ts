// ISSUES: 0 open, last updated 2025-08-24 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Test Command Types - Shared type definitions
 * 
 * Defines parameters and results for test command execution
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

export interface TestParams {
  context: JTAGContext;
  sessionId: UUID;
  
  // Test execution parameters
  file?: string;           // Specific test file to run
  _?: string[];           // Positional arguments (test files)
  timeout?: number;       // Timeout in milliseconds (default: 300000 = 5 minutes)
}

export interface TestResult {
  context: JTAGContext;
  sessionId: UUID;
  success: boolean;
  output: string;         // Test execution output
  duration: number;       // Execution time in milliseconds
  command: string;        // Actual command executed
  error?: Error;         // Error if failed
}

/**
 * Create default test parameters
 */
export function createTestParams(
  context: JTAGContext,
  sessionId: UUID, 
  overrides: Partial<TestParams> = {}
): TestParams {
  return {
    context,
    sessionId,
    timeout: 300000, // 5 minutes default
    ...overrides
  };
}

/**
 * Create test result
 */
export function createTestResult(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<TestResult, 'context' | 'sessionId'>
): TestResult {
  return {
    context,
    sessionId,
    ...data
  };
}