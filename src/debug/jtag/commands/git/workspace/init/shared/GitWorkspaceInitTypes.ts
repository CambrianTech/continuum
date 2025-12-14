/**
 * Git Workspace Init Command - Shared Types
 *
 * Initialize git workspace for persona collaboration with isolated worktree
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';

/**
 * Git Workspace Init Command Parameters
 */
export interface GitWorkspaceInitParams extends CommandParams {
  // Branch name for the workspace (defaults to {persona-name}/{timestamp})
  branch?: string;
  // Persona ID to create workspace for (auto-detected if not provided)
  personaId?: string;
  // Paths to checkout with sparse-checkout (REQUIRED - e.g., ["docs/", "src/api/"])
  paths: string[];
}

/**
 * Factory function for creating GitWorkspaceInitParams
 */
export const createGitWorkspaceInitParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Branch name for the workspace (defaults to {persona-name}/{timestamp})
    branch?: string;
    // Persona ID to create workspace for (auto-detected if not provided)
    personaId?: string;
    // Paths to checkout with sparse-checkout (REQUIRED - e.g., ["docs/", "src/api/"])
    paths: string[];
  }
): GitWorkspaceInitParams => createPayload(context, sessionId, {
  branch: data.branch ?? '',
  personaId: data.personaId ?? '',
  paths: data.paths
});

/**
 * Git Workspace Init Command Result
 */
export interface GitWorkspaceInitResult extends CommandResult {
  success: boolean;
  // Unique workspace ID (UUID)
  workspaceId: string;
  // Short ID for workspace (#abc123 format)
  shortId: string;
  // Absolute path to workspace directory
  workspacePath: string;
  // Git branch name
  branch: string;
  error?: JTAGError;
}

/**
 * Factory function for creating GitWorkspaceInitResult with defaults
 */
export const createGitWorkspaceInitResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Unique workspace ID (UUID)
    workspaceId?: string;
    // Short ID for workspace (#abc123 format)
    shortId?: string;
    // Absolute path to workspace directory
    workspacePath?: string;
    // Git branch name
    branch?: string;
    error?: JTAGError;
  }
): GitWorkspaceInitResult => createPayload(context, sessionId, {
  workspaceId: data.workspaceId ?? '',
  shortId: data.shortId ?? '',
  workspacePath: data.workspacePath ?? '',
  branch: data.branch ?? '',
  ...data
});

/**
 * Smart Git Workspace Init-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGitWorkspaceInitResultFromParams = (
  params: GitWorkspaceInitParams,
  differences: Omit<GitWorkspaceInitResult, 'context' | 'sessionId'>
): GitWorkspaceInitResult => transformPayload(params, differences);
