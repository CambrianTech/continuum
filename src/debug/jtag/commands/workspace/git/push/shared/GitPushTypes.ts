/**
 * Git Push Command - Shared Types
 *
 * Push workspace branch to remote repository
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Git Push Command Parameters
 */
export interface GitPushParams extends CommandParams {
  // Path to workspace (auto-detected from context if not provided)
  workspacePath?: string;
  // Remote name (defaults to 'origin')
  remote?: string;
}

/**
 * Factory function for creating GitPushParams
 */
export const createGitPushParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to workspace (auto-detected from context if not provided)
    workspacePath?: string;
    // Remote name (defaults to 'origin')
    remote?: string;
  }
): GitPushParams => createPayload(context, sessionId, {
  workspacePath: data.workspacePath ?? '',
  remote: data.remote ?? '',
  ...data
});

/**
 * Git Push Command Result
 */
export interface GitPushResult extends CommandResult {
  success: boolean;
  // Branch that was pushed
  branch: string;
  // Remote repository pushed to
  remote: string;
  // Number of commits pushed
  commitsPushed: number;
  error?: JTAGError;
}

/**
 * Factory function for creating GitPushResult with defaults
 */
export const createGitPushResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Branch that was pushed
    branch?: string;
    // Remote repository pushed to
    remote?: string;
    // Number of commits pushed
    commitsPushed?: number;
    error?: JTAGError;
  }
): GitPushResult => createPayload(context, sessionId, {
  branch: data.branch ?? '',
  remote: data.remote ?? '',
  commitsPushed: data.commitsPushed ?? 0,
  ...data
});

/**
 * Smart Git Push-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGitPushResultFromParams = (
  params: GitPushParams,
  differences: Omit<GitPushResult, 'context' | 'sessionId'>
): GitPushResult => transformPayload(params, differences);
