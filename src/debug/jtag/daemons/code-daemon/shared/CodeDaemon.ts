/**
 * CodeDaemon - Static interface for workspace-scoped code operations
 *
 * Environment-agnostic interface. All implementation is in server/.
 * All operations go through Rust IPC backend with per-persona isolation.
 */

import type {
  WorkspaceEditMode,
  WorkspaceWriteResult,
  WorkspaceReadResult,
  WorkspaceSearchResult,
  WorkspaceTreeResult,
  WorkspaceUndoResult,
  WorkspaceHistoryResult,
  WorkspaceGitStatusInfo,
} from './CodeDaemonTypes';

/**
 * CodeDaemon - Static API for workspace-scoped code operations
 *
 * All methods throw error if not initialized or called from wrong environment.
 * Implementation is in server/CodeDaemonServer.ts
 */
export class CodeDaemon {

  // ========================================================================
  // Workspace-Scoped Operations (Rust IPC backed, per-persona isolation)
  // ========================================================================

  /**
   * Initialize a per-persona workspace with file engine and change graph.
   * Must be called before any other workspace operations for this persona.
   */
  static async createWorkspace(personaId: string, workspaceRoot: string, readRoots?: string[]): Promise<void> {
    throw new Error('CodeDaemon.createWorkspace() must be implemented by server');
  }

  /**
   * Read a file from the persona's workspace.
   */
  static async workspaceRead(personaId: string, filePath: string, startLine?: number, endLine?: number): Promise<WorkspaceReadResult> {
    throw new Error('CodeDaemon.workspaceRead() must be implemented by server');
  }

  /**
   * Write or create a file in the persona's workspace.
   */
  static async workspaceWrite(personaId: string, filePath: string, content: string, description?: string): Promise<WorkspaceWriteResult> {
    throw new Error('CodeDaemon.workspaceWrite() must be implemented by server');
  }

  /**
   * Edit a file using one of four edit modes.
   */
  static async workspaceEdit(personaId: string, filePath: string, editMode: WorkspaceEditMode, description?: string): Promise<WorkspaceWriteResult> {
    throw new Error('CodeDaemon.workspaceEdit() must be implemented by server');
  }

  /**
   * Delete a file from the persona's workspace.
   */
  static async workspaceDelete(personaId: string, filePath: string, description?: string): Promise<WorkspaceWriteResult> {
    throw new Error('CodeDaemon.workspaceDelete() must be implemented by server');
  }

  /**
   * Preview an edit as a unified diff without applying it.
   */
  static async workspaceDiff(personaId: string, filePath: string, editMode: WorkspaceEditMode): Promise<{ success: boolean; unified: string }> {
    throw new Error('CodeDaemon.workspaceDiff() must be implemented by server');
  }

  /**
   * Undo a specific change or the last N changes.
   */
  static async workspaceUndo(personaId: string, changeId?: string, count?: number): Promise<WorkspaceUndoResult> {
    throw new Error('CodeDaemon.workspaceUndo() must be implemented by server');
  }

  /**
   * Get change history for a file or entire workspace.
   */
  static async workspaceHistory(personaId: string, filePath?: string, limit?: number): Promise<WorkspaceHistoryResult> {
    throw new Error('CodeDaemon.workspaceHistory() must be implemented by server');
  }

  /**
   * Search for a regex pattern across workspace files.
   */
  static async workspaceSearch(personaId: string, pattern: string, fileGlob?: string, maxResults?: number): Promise<WorkspaceSearchResult> {
    throw new Error('CodeDaemon.workspaceSearch() must be implemented by server');
  }

  /**
   * Generate a directory tree for the workspace.
   */
  static async workspaceTree(personaId: string, path?: string, maxDepth?: number, includeHidden?: boolean): Promise<WorkspaceTreeResult> {
    throw new Error('CodeDaemon.workspaceTree() must be implemented by server');
  }

  /**
   * Get git status for the workspace.
   */
  static async workspaceGitStatus(personaId: string): Promise<WorkspaceGitStatusInfo> {
    throw new Error('CodeDaemon.workspaceGitStatus() must be implemented by server');
  }

  /**
   * Get git diff for the workspace.
   */
  static async workspaceGitDiff(personaId: string, staged?: boolean): Promise<{ success: boolean; diff: string }> {
    throw new Error('CodeDaemon.workspaceGitDiff() must be implemented by server');
  }

  /**
   * Get git log for the workspace (last N commits).
   */
  static async workspaceGitLog(personaId: string, count?: number): Promise<{ success: boolean; log: string }> {
    throw new Error('CodeDaemon.workspaceGitLog() must be implemented by server');
  }

  /**
   * Stage files for commit in the workspace.
   */
  static async workspaceGitAdd(personaId: string, paths: string[]): Promise<{ staged: string[] }> {
    throw new Error('CodeDaemon.workspaceGitAdd() must be implemented by server');
  }

  /**
   * Create a git commit in the workspace.
   */
  static async workspaceGitCommit(personaId: string, message: string): Promise<{ hash: string }> {
    throw new Error('CodeDaemon.workspaceGitCommit() must be implemented by server');
  }

  /**
   * Push the workspace branch to remote.
   */
  static async workspaceGitPush(personaId: string, remote?: string, branch?: string): Promise<{ output: string }> {
    throw new Error('CodeDaemon.workspaceGitPush() must be implemented by server');
  }
}
