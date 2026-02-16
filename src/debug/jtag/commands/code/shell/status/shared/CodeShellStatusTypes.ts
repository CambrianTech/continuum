/**
 * Code Shell Status Command - Shared Types
 *
 * Get shell session info for the persona's workspace — current working directory, active and total execution count. No parameters required (userId auto-injected).
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Code Shell Status Command Parameters
 */
export interface CodeShellStatusParams extends CommandParams {
  _noParams?: never; // Marker to avoid empty interface
}

/**
 * Factory function for creating CodeShellStatusParams
 */
export const createCodeShellStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Record<string, never>
): CodeShellStatusParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Code Shell Status Command Result
 */
export interface CodeShellStatusResult extends CommandResult {
  success: boolean;
  // Shell session identifier (named shellSessionId to avoid collision with base CommandResult.sessionId)
  shellSessionId: string;
  // Persona that owns this shell session
  personaId: string;
  // Current working directory of the shell session
  cwd: string;
  // Root directory of the workspace
  workspaceRoot: string;
  // Number of currently running executions
  activeExecutions: number;
  // Total number of executions (running + completed)
  totalExecutions: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeShellStatusResult with defaults
 */
export const createCodeShellStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Shell session identifier
    shellSessionId?: string;
    // Persona that owns this shell session
    personaId?: string;
    // Current working directory of the shell session
    cwd?: string;
    // Root directory of the workspace
    workspaceRoot?: string;
    // Number of currently running executions
    activeExecutions?: number;
    // Total number of executions (running + completed)
    totalExecutions?: number;
    error?: JTAGError;
  }
): CodeShellStatusResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  shellSessionId: data.shellSessionId ?? '',
  personaId: data.personaId ?? '',
  cwd: data.cwd ?? '',
  workspaceRoot: data.workspaceRoot ?? '',
  activeExecutions: data.activeExecutions ?? 0,
  totalExecutions: data.totalExecutions ?? 0,
  ...data
});

/**
 * Smart Code Shell Status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeShellStatusResultFromParams = (
  params: CodeShellStatusParams,
  differences: Omit<CodeShellStatusResult, 'context' | 'sessionId'>
): CodeShellStatusResult => transformPayload(params, differences);

/**
 * Code Shell Status — Type-safe command executor
 *
 * Usage:
 *   import { CodeShellStatus } from '...shared/CodeShellStatusTypes';
 *   const result = await CodeShellStatus.execute({ ... });
 */
export const CodeShellStatus = {
  execute(params: CommandInput<CodeShellStatusParams>): Promise<CodeShellStatusResult> {
    return Commands.execute<CodeShellStatusParams, CodeShellStatusResult>('code/shell/status', params as Partial<CodeShellStatusParams>);
  },
  commandName: 'code/shell/status' as const,
} as const;
