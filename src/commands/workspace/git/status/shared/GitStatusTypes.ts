/**
 * Git Status Command - Shared Types
 *
 * Show git workspace status and changes
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Git Status Command Parameters
 */
export interface GitStatusParams extends CommandParams {
  // Path to workspace (auto-detected from context if not provided)
  workspacePath?: string;
}

/**
 * Factory function for creating GitStatusParams
 */
export const createGitStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to workspace (auto-detected from context if not provided)
    workspacePath?: string;
  }
): GitStatusParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  workspacePath: data.workspacePath ?? '',
  ...data
});

/**
 * Git Status Command Result
 */
export interface GitStatusResult extends CommandResult {
  success: boolean;
  // Current branch name
  branch: string;
  // List of modified files
  modified: string[];
  // List of staged files
  staged: string[];
  // List of untracked files
  untracked: string[];
  // Number of commits ahead of remote
  commitsAhead: number;
  error?: JTAGError;
}

/**
 * Factory function for creating GitStatusResult with defaults
 */
export const createGitStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Current branch name
    branch?: string;
    // List of modified files
    modified?: string[];
    // List of staged files
    staged?: string[];
    // List of untracked files
    untracked?: string[];
    // Number of commits ahead of remote
    commitsAhead?: number;
    error?: JTAGError;
  }
): GitStatusResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  branch: data.branch ?? '',
  modified: data.modified ?? [],
  staged: data.staged ?? [],
  untracked: data.untracked ?? [],
  commitsAhead: data.commitsAhead ?? 0,
  ...data
});

/**
 * Smart Git Status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGitStatusResultFromParams = (
  params: GitStatusParams,
  differences: Omit<GitStatusResult, 'context' | 'sessionId'>
): GitStatusResult => transformPayload(params, differences);

/**
 * GitStatus — Type-safe command executor
 *
 * Usage:
 *   import { GitStatus } from '...shared/GitStatusTypes';
 *   const result = await GitStatus.execute({ ... });
 */
export const GitStatus = {
  execute(params: CommandInput<GitStatusParams>): Promise<GitStatusResult> {
    return Commands.execute<GitStatusParams, GitStatusResult>('workspace/git/status', params as Partial<GitStatusParams>);
  },
  commandName: 'workspace/git/status' as const,
} as const;
