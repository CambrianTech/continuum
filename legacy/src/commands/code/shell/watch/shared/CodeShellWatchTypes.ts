/**
 * Code Shell Watch Command - Shared Types
 *
 * Watch a shell execution for new output. Blocks until output is available — no timeout, no polling.
 * Returns classified output lines filtered through sentinel rules. Call in a loop until finished is true.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ClassifiedLine } from '@shared/generated/code/ClassifiedLine';

/**
 * Code Shell Watch Command Parameters
 */
export interface CodeShellWatchParams extends CommandParams {
  /** Execution handle from shell/exec */
  executionId: string;
}

/**
 * Factory function for creating CodeShellWatchParams
 */
export const createCodeShellWatchParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    executionId: string;
  }
): CodeShellWatchParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Code Shell Watch Command Result
 */
export interface CodeShellWatchResult extends CommandResult {
  success: boolean;
  /** Echo of the execution handle */
  executionId: string;
  /** New output lines since last watch call (classified and filtered) */
  lines: ClassifiedLine[];
  /** True when execution is complete */
  finished: boolean;
  /** Process exit code (present when finished) */
  exitCode?: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeShellWatchResult with defaults
 */
export const createCodeShellWatchResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    executionId?: string;
    lines?: ClassifiedLine[];
    finished?: boolean;
    exitCode?: number;
    error?: JTAGError;
  }
): CodeShellWatchResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  executionId: data.executionId ?? '',
  lines: data.lines ?? [],
  finished: data.finished ?? false,
  exitCode: data.exitCode,
  ...data
});

/**
 * Smart Code Shell Watch-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeShellWatchResultFromParams = (
  params: CodeShellWatchParams,
  differences: Omit<CodeShellWatchResult, 'context' | 'sessionId'>
): CodeShellWatchResult => transformPayload(params, differences);

/**
 * Code Shell Watch — Type-safe command executor
 *
 * Usage:
 *   import { CodeShellWatch } from '...shared/CodeShellWatchTypes';
 *   const result = await CodeShellWatch.execute({ ... });
 */
export const CodeShellWatch = {
  execute(params: CommandInput<CodeShellWatchParams>): Promise<CodeShellWatchResult> {
    return Commands.execute<CodeShellWatchParams, CodeShellWatchResult>('code/shell/watch', params as Partial<CodeShellWatchParams>);
  },
  commandName: 'code/shell/watch' as const,
} as const;
