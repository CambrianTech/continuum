/**
 * Git Commit Command - Shared Types
 *
 * Commit changes in git workspace with persona identity
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Git Commit Command Parameters
 */
export interface GitCommitParams extends CommandParams {
  // Commit message
  message: string;
  // Path to workspace (auto-detected from context if not provided)
  workspacePath?: string;
  // Specific files to commit (defaults to all changed files)
  files?: string[];
}

/**
 * Factory function for creating GitCommitParams
 */
export const createGitCommitParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Commit message
    message: string;
    // Path to workspace (auto-detected from context if not provided)
    workspacePath?: string;
    // Specific files to commit (defaults to all changed files)
    files?: string[];
  }
): GitCommitParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  workspacePath: data.workspacePath ?? '',
  files: data.files ?? undefined,
  ...data
});

/**
 * Git Commit Command Result
 */
export interface GitCommitResult extends CommandResult {
  success: boolean;
  // Git commit hash
  commitHash: string;
  // Short commit hash (7 chars)
  shortHash: string;
  // Number of files committed
  filesCommitted: number;
  error?: JTAGError;
}

/**
 * Factory function for creating GitCommitResult with defaults
 */
export const createGitCommitResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Git commit hash
    commitHash?: string;
    // Short commit hash (7 chars)
    shortHash?: string;
    // Number of files committed
    filesCommitted?: number;
    error?: JTAGError;
  }
): GitCommitResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  commitHash: data.commitHash ?? '',
  shortHash: data.shortHash ?? '',
  filesCommitted: data.filesCommitted ?? 0,
  ...data
});

/**
 * Smart Git Commit-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGitCommitResultFromParams = (
  params: GitCommitParams,
  differences: Omit<GitCommitResult, 'context' | 'sessionId'>
): GitCommitResult => transformPayload(params, differences);

/**
 * GitCommit — Type-safe command executor
 *
 * Usage:
 *   import { GitCommit } from '...shared/GitCommitTypes';
 *   const result = await GitCommit.execute({ ... });
 */
export const GitCommit = {
  execute(params: CommandInput<GitCommitParams>): Promise<GitCommitResult> {
    return Commands.execute<GitCommitParams, GitCommitResult>('workspace/git/commit', params as Partial<GitCommitParams>);
  },
  commandName: 'workspace/git/commit' as const,
} as const;
