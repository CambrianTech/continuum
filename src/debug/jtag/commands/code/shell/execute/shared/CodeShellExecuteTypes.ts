/**
 * Code Shell Execute Command - Shared Types
 *
 * Execute a shell command in the persona's workspace. Async mode (default) returns execution handle immediately — use code/shell/watch to stream output. Sync mode (wait=true) blocks until completion and returns full stdout/stderr.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ShellExecutionStatus } from '@shared/generated/code/ShellExecutionStatus';

/**
 * Code Shell Execute Command Parameters
 */
export interface CodeShellExecuteParams extends CommandParams {
  // Shell command to execute (e.g., "npm run build", "cargo test", "xcodebuild")
  cmd: string;
  // Wait for completion before returning (default: false — returns handle immediately)
  wait?: boolean;
  // Timeout in milliseconds (default: 30000 for sync, no limit for async)
  timeoutMs?: number;
}

/**
 * Factory function for creating CodeShellExecuteParams
 */
export const createCodeShellExecuteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Shell command to execute (e.g., "npm run build", "cargo test", "xcodebuild")
    cmd: string;
    // Wait for completion before returning (default: false — returns handle immediately)
    wait?: boolean;
    // Timeout in milliseconds (default: 30000 for sync, no limit for async)
    timeoutMs?: number;
  }
): CodeShellExecuteParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  wait: data.wait ?? false,
  timeoutMs: data.timeoutMs ?? 0,
  ...data
});

/**
 * Code Shell Execute Command Result
 */
export interface CodeShellExecuteResult extends CommandResult {
  success: boolean;
  /** Execution handle — use with code/shell/watch, code/shell/kill */
  executionId: string;
  /** Execution status: running, completed, failed, timed_out, killed */
  status: ShellExecutionStatus;
  /** Full stdout (only present when wait=true and execution completed) */
  stdout?: string;
  /** Full stderr (only present when wait=true and execution completed) */
  stderr?: string;
  /** Process exit code (only present when execution completed) */
  exitCode?: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeShellExecuteResult with defaults
 */
export const createCodeShellExecuteResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Execution handle — use with code/shell/watch, code/shell/kill
    executionId?: string;
    // Execution status: running, completed, failed, timed_out, killed
    status?: ShellExecutionStatus;
    // Full stdout (only present when wait=true and execution completed)
    stdout?: string;
    // Full stderr (only present when wait=true and execution completed)
    stderr?: string;
    // Process exit code (only present when execution completed)
    exitCode?: number;
    error?: JTAGError;
  }
): CodeShellExecuteResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  executionId: data.executionId ?? '',
  status: data.status ?? 'running' as ShellExecutionStatus,
  stdout: data.stdout,
  stderr: data.stderr,
  exitCode: data.exitCode,
  ...data
});

/**
 * Smart Code Shell Execute-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeShellExecuteResultFromParams = (
  params: CodeShellExecuteParams,
  differences: Omit<CodeShellExecuteResult, 'context' | 'sessionId'>
): CodeShellExecuteResult => transformPayload(params, differences);

/**
 * Code Shell Execute — Type-safe command executor
 *
 * Usage:
 *   import { CodeShellExecute } from '...shared/CodeShellExecuteTypes';
 *   const result = await CodeShellExecute.execute({ ... });
 */
export const CodeShellExecute = {
  execute(params: CommandInput<CodeShellExecuteParams>): Promise<CodeShellExecuteResult> {
    return Commands.execute<CodeShellExecuteParams, CodeShellExecuteResult>('code/shell/execute', params as Partial<CodeShellExecuteParams>);
  },
  commandName: 'code/shell/execute' as const,
} as const;
