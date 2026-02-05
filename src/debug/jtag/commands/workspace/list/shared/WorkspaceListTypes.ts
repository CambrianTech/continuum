/**
 * Workspace List Command - Shared Types
 *
 * List all persona workspaces across the team — worktree paths, git branches,
 * modified files, shell activity. Scans both in-memory active workspaces and
 * persisted git worktrees on disk.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Workspace List Command Parameters
 */
export interface WorkspaceListParams extends CommandParams {
  /** Filter to a specific persona's workspaces (by uniqueId). If omitted, returns all. */
  personaId?: string;
  /** Include git status (branch, modified files, staged) for each workspace. Defaults to true. */
  includeGitStatus?: boolean;
}

/**
 * Per-workspace information returned by workspace/list.
 */
export interface WorkspaceInfo {
  /** Persona uniqueId that owns this workspace (e.g., 'deepseek', 'together') */
  personaId: string;
  /** Task slug within the persona's worktree directory */
  taskSlug: string;
  /** Absolute path to the worktree directory */
  worktreeDir: string;
  /** Git branch checked out in this worktree */
  branch: string;
  /** Whether this workspace is currently active in the server's in-memory state */
  active: boolean;
  /** Workspace mode (detected from structure) */
  mode: 'project' | 'worktree' | 'sandbox' | 'unknown';
  /** Git status (populated when includeGitStatus is true) */
  git?: WorkspaceGitStatus;
}

/**
 * Git status for a single workspace.
 */
export interface WorkspaceGitStatus {
  /** List of modified (unstaged) files */
  modified: string[];
  /** List of staged files */
  staged: string[];
  /** List of untracked files */
  untracked: string[];
  /** Number of commits ahead of the tracking branch */
  commitsAhead: number;
  /** HEAD commit short hash */
  headCommit: string;
  /** HEAD commit message (first line) */
  headMessage: string;
}

/**
 * Factory function for creating WorkspaceListParams
 */
export const createWorkspaceListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    personaId?: string;
    includeGitStatus?: boolean;
  }
): WorkspaceListParams => createPayload(context, sessionId, {
  personaId: data.personaId ?? '',
  includeGitStatus: data.includeGitStatus ?? true,
  ...data
});

/**
 * Workspace List Command Result
 */
export interface WorkspaceListResult extends CommandResult {
  success: boolean;
  /** Array of workspace info objects for each discovered workspace */
  workspaces: WorkspaceInfo[];
  /** Total number of workspaces found */
  totalCount: number;
  /** Number of workspaces currently active in memory (server session) */
  activeCount: number;
  error?: JTAGError;
}

/**
 * Factory function for creating WorkspaceListResult with defaults
 */
export const createWorkspaceListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    workspaces?: WorkspaceInfo[];
    totalCount?: number;
    activeCount?: number;
    error?: JTAGError;
  }
): WorkspaceListResult => createPayload(context, sessionId, {
  workspaces: data.workspaces ?? [],
  totalCount: data.totalCount ?? 0,
  activeCount: data.activeCount ?? 0,
  ...data
});

/**
 * Smart Workspace List-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createWorkspaceListResultFromParams = (
  params: WorkspaceListParams,
  differences: Omit<WorkspaceListResult, 'context' | 'sessionId'>
): WorkspaceListResult => transformPayload(params, differences);

/**
 * WorkspaceList — Type-safe command executor
 *
 * Usage:
 *   import { WorkspaceList } from '...shared/WorkspaceListTypes';
 *   const result = await WorkspaceList.execute({ ... });
 */
export const WorkspaceList = {
  execute(params: CommandInput<WorkspaceListParams>): Promise<WorkspaceListResult> {
    return Commands.execute<WorkspaceListParams, WorkspaceListResult>('workspace/list', params as Partial<WorkspaceListParams>);
  },
  commandName: 'workspace/list' as const,
} as const;
