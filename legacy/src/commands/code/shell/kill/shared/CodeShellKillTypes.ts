/**
 * Code Shell Kill Command - Shared Types
 *
 * Kill a running shell execution. Use the executionId returned by code/shell/execute to identify the target.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Code Shell Kill Command Parameters
 */
export interface CodeShellKillParams extends CommandParams {
  // Execution handle to kill (from code/shell/execute)
  executionId: string;
}

/**
 * Factory function for creating CodeShellKillParams
 */
export const createCodeShellKillParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Execution handle to kill (from code/shell/execute)
    executionId: string;
  }
): CodeShellKillParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Code Shell Kill Command Result
 */
export interface CodeShellKillResult extends CommandResult {
  success: boolean;
  // Echo of the killed execution handle
  executionId: string;
  // Whether the execution was successfully killed
  killed: boolean;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeShellKillResult with defaults
 */
export const createCodeShellKillResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Echo of the killed execution handle
    executionId?: string;
    // Whether the execution was successfully killed
    killed?: boolean;
    error?: JTAGError;
  }
): CodeShellKillResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  executionId: data.executionId ?? '',
  killed: data.killed ?? false,
  ...data
});

/**
 * Smart Code Shell Kill-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeShellKillResultFromParams = (
  params: CodeShellKillParams,
  differences: Omit<CodeShellKillResult, 'context' | 'sessionId'>
): CodeShellKillResult => transformPayload(params, differences);

/**
 * Code Shell Kill — Type-safe command executor
 *
 * Usage:
 *   import { CodeShellKill } from '...shared/CodeShellKillTypes';
 *   const result = await CodeShellKill.execute({ ... });
 */
export const CodeShellKill = {
  execute(params: CommandInput<CodeShellKillParams>): Promise<CodeShellKillResult> {
    return Commands.execute<CodeShellKillParams, CodeShellKillResult>('code/shell/kill', params as Partial<CodeShellKillParams>);
  },
  commandName: 'code/shell/kill' as const,
} as const;
