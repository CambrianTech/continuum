// ISSUES: 0 open, last updated 2025-08-24 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Test Command Types - Type definitions for JTAG test command
 * 
 * Simple test command that uses server-side execution to run npm scripts
 * or tsx files without duplicating complex setup logic.
 */

export interface TestParams {
  file?: string;    // Test file to run with tsx
  script?: string;  // npm script to run
  timeout?: number; // Timeout in milliseconds
}

export interface TestResult {
  success: boolean;
  output: string;
  command: string;
  duration: number;
}

export function createTestResult(
  success: boolean,
  command: string,
  duration: number,
  output: string
): TestResult {
  return {
    success,
    command,
    duration,
    output
  };
}