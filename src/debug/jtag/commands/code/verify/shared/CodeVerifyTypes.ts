/**
 * Code Verify Command - Shared Types
 *
 * Run TypeScript compilation checks and optionally execute tests against a persona workspace.
 * Returns structured errors with file, line, column, and message for each issue found.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Parsed TypeScript compilation error
 */
export interface TypeScriptError {
  file: string;
  line: number;
  column: number;
  code: string;
  message: string;
}

/**
 * Code Verify Command Parameters
 */
export interface CodeVerifyParams extends CommandParams {
  /** Run TypeScript compilation check (default: true) */
  typeCheck?: boolean;
  /** Specific test files to run via vitest (optional) */
  testFiles?: string[];
  /** Working directory override — bypasses workspace resolution */
  cwd?: string;
}

/**
 * Factory function for creating CodeVerifyParams
 */
export const createCodeVerifyParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    typeCheck?: boolean;
    testFiles?: string[];
    cwd?: string;
  }
): CodeVerifyParams => createPayload(context, sessionId, {
  typeCheck: data.typeCheck ?? true,
  testFiles: data.testFiles ?? [],
  cwd: data.cwd ?? '',
  ...data
});

/**
 * Test execution result
 */
export interface TestResult {
  passed: boolean;
  total: number;
  passedCount: number;
  failedCount: number;
  failures: string[];
}

/**
 * Code Verify Command Result
 */
export interface CodeVerifyResult extends CommandResult {
  success: boolean;
  /** TypeScript compilation result (if typeCheck was requested) */
  typeCheck?: {
    passed: boolean;
    errorCount: number;
    errors: TypeScriptError[];
  };
  /** Test execution result (if testFiles were specified) */
  tests?: TestResult;
  /** Total verification time in milliseconds */
  durationMs: number;
  /** Raw compiler/test output */
  output: string;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeVerifyResult with defaults
 */
export const createCodeVerifyResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    typeCheck?: CodeVerifyResult['typeCheck'];
    tests?: TestResult;
    durationMs?: number;
    output?: string;
    error?: JTAGError;
  }
): CodeVerifyResult => createPayload(context, sessionId, {
  durationMs: data.durationMs ?? 0,
  output: data.output ?? '',
  ...data
});

/**
 * Smart Code Verify-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeVerifyResultFromParams = (
  params: CodeVerifyParams,
  differences: Omit<CodeVerifyResult, 'context' | 'sessionId'>
): CodeVerifyResult => transformPayload(params, differences);

/**
 * Code Verify — Type-safe command executor
 *
 * Usage:
 *   import { CodeVerify } from '...shared/CodeVerifyTypes';
 *   const result = await CodeVerify.execute({ typeCheck: true });
 */
export const CodeVerify = {
  execute(params: CommandInput<CodeVerifyParams>): Promise<CodeVerifyResult> {
    return Commands.execute<CodeVerifyParams, CodeVerifyResult>('code/verify', params as Partial<CodeVerifyParams>);
  },
  commandName: 'code/verify' as const,
} as const;
