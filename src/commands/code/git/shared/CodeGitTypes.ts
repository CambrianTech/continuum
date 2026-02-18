/**
 * Code Git Command - Shared Types
 *
 * Workspace-scoped git operations for the coding agent pipeline.
 * Operations: status, diff, log, add, commit, push.
 * All operations are routed through the Rust IPC backend for per-persona workspace isolation.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Supported git operations.
 */
export type GitOperation = 'status' | 'diff' | 'log' | 'add' | 'commit' | 'push';

/**
 * Code Git Command Parameters
 */
export interface CodeGitParams extends CommandParams {
  /** Persona/workspace handle */
  userId: string;
  /** Git operation to perform */
  operation: string;
  /** File paths to stage (for 'add' operation) */
  paths?: string[];
  /** Commit message (for 'commit' operation) */
  message?: string;
  /** Remote name (for 'push' operation, default: 'origin') */
  remote?: string;
  /** Branch name (for 'push' operation) */
  branch?: string;
  /** Show staged changes (for 'diff' operation) */
  staged?: boolean;
  /** Number of commits to show (for 'log' operation, default: 10) */
  count?: number;
}

/**
 * Factory function for creating CodeGitParams
 */
export const createCodeGitParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    userId: string;
    operation: string;
    paths?: string[];
    message?: string;
    remote?: string;
    branch?: string;
    staged?: boolean;
    count?: number;
  }
): CodeGitParams => createPayload(context, sessionId, {
  paths: data.paths ?? [],
  message: data.message ?? '',
  remote: data.remote ?? '',
  branch: data.branch ?? '',
  staged: data.staged ?? false,
  count: data.count ?? 0,
  ...data
});

/**
 * Git status information
 */
export interface GitStatusInfo {
  branch?: string;
  modified: string[];
  added: string[];
  deleted: string[];
  untracked: string[];
}

/**
 * Code Git Command Result
 */
export interface CodeGitResult extends CommandResult {
  success: boolean;
  /** Which operation was performed */
  operation: string;
  /** Git status info (for 'status' operation) */
  status?: GitStatusInfo;
  /** Diff output (for 'diff' operation) */
  diff?: string;
  /** Log output (for 'log' operation) */
  log?: string;
  /** Staged file paths (for 'add' operation) */
  staged?: string[];
  /** Commit hash (for 'commit' operation) */
  commitHash?: string;
  /** Push output (for 'push' operation) */
  pushOutput?: string;
  /** Human-readable summary */
  summary: string;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeGitResult with defaults
 */
export const createCodeGitResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    operation: string;
    status?: GitStatusInfo;
    diff?: string;
    log?: string;
    staged?: string[];
    commitHash?: string;
    pushOutput?: string;
    summary?: string;
    error?: JTAGError;
  }
): CodeGitResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  summary: data.summary ?? '',
  ...data
});

/**
 * Smart result inheritance from params
 */
export const createCodeGitResultFromParams = (
  params: CodeGitParams,
  differences: Omit<CodeGitResult, 'context' | 'sessionId'>
): CodeGitResult => transformPayload(params, differences);

/**
 * Code Git - Type-safe command executor
 *
 * Usage:
 *   import { CodeGit } from '...shared/CodeGitTypes';
 *   const result = await CodeGit.execute({ userId: 'persona-id', operation: 'status' });
 */
export const CodeGit = {
  execute(params: CommandInput<CodeGitParams>): Promise<CodeGitResult> {
    return Commands.execute<CodeGitParams, CodeGitResult>('code/git', params as Partial<CodeGitParams>);
  },
  commandName: 'code/git' as const,
} as const;
