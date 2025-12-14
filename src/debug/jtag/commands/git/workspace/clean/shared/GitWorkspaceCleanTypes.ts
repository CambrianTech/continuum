/**
 * Git Workspace Clean Command - Shared Types
 *
 * Clean up git workspace and remove worktree
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';

/**
 * Git Workspace Clean Command Parameters
 */
export interface GitWorkspaceCleanParams extends CommandParams {
  // Path to workspace (auto-detected from context if not provided)
  workspacePath?: string;
  // Force cleanup even with uncommitted changes (defaults to false)
  force?: boolean;
  // Delete the branch after cleanup (defaults to false)
  deleteBranch?: boolean;
}

/**
 * Factory function for creating GitWorkspaceCleanParams
 */
export const createGitWorkspaceCleanParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to workspace (auto-detected from context if not provided)
    workspacePath?: string;
    // Force cleanup even with uncommitted changes (defaults to false)
    force?: boolean;
    // Delete the branch after cleanup (defaults to false)
    deleteBranch?: boolean;
  }
): GitWorkspaceCleanParams => createPayload(context, sessionId, {
  workspacePath: data.workspacePath ?? '',
  force: data.force ?? false,
  deleteBranch: data.deleteBranch ?? false,
  ...data
});

/**
 * Git Workspace Clean Command Result
 */
export interface GitWorkspaceCleanResult extends CommandResult {
  success: boolean;
  // Whether workspace was removed
  workspaceRemoved: boolean;
  // Whether branch was deleted
  branchDeleted: boolean;
  error?: JTAGError;
}

/**
 * Factory function for creating GitWorkspaceCleanResult with defaults
 */
export const createGitWorkspaceCleanResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether workspace was removed
    workspaceRemoved?: boolean;
    // Whether branch was deleted
    branchDeleted?: boolean;
    error?: JTAGError;
  }
): GitWorkspaceCleanResult => createPayload(context, sessionId, {
  workspaceRemoved: data.workspaceRemoved ?? false,
  branchDeleted: data.branchDeleted ?? false,
  ...data
});

/**
 * Smart Git Workspace Clean-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGitWorkspaceCleanResultFromParams = (
  params: GitWorkspaceCleanParams,
  differences: Omit<GitWorkspaceCleanResult, 'context' | 'sessionId'>
): GitWorkspaceCleanResult => transformPayload(params, differences);
