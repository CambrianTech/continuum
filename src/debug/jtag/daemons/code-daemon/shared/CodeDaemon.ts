/**
 * CodeDaemon - Static interface for code operations
 *
 * Environment-agnostic interface. All implementation is in server/.
 */

import type {
  CodeReadOptions,
  CodeReadResult,
  CodeSearchOptions,
  CodeSearchResult,
  GitLogOptions,
  GitLogResult,
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
 * CodeDaemon - Static API for code operations
 *
 * All methods throw error if not initialized or called from wrong environment.
 * Implementation is in server/CodeDaemonImpl.ts
 */
export class CodeDaemon {
  /**
   * Read a file (STATIC METHOD - public API)
   */
  static async readFile(path: string, options?: CodeReadOptions): Promise<CodeReadResult> {
    throw new Error('CodeDaemon.readFile() must be implemented by server');
  }

  /**
   * Search code (STATIC METHOD - public API)
   */
  static async searchCode(pattern: string, options?: CodeSearchOptions): Promise<CodeSearchResult> {
    throw new Error('CodeDaemon.searchCode() must be implemented by server');
  }

  /**
   * Get git log (STATIC METHOD - public API)
   */
  static async getGitLog(options?: GitLogOptions): Promise<GitLogResult> {
    throw new Error('CodeDaemon.getGitLog() must be implemented by server');
  }

  /**
   * Clear file cache (STATIC METHOD)
   */
  static clearCache(): void {
    throw new Error('CodeDaemon.clearCache() must be implemented by server');
  }

  /**
   * Get cache stats (STATIC METHOD)
   */
  static getCacheStats(): { entries: number; size: number } {
    throw new Error('CodeDaemon.getCacheStats() must be implemented by server');
  }

  /**
   * Get repository root (STATIC METHOD)
   */
  static getRepositoryRoot(): string {
    throw new Error('CodeDaemon.getRepositoryRoot() must be implemented by server');
  }

  /**
   * Check if initialized (STATIC METHOD)
   */
  static isInitialized(): boolean {
    return false; // Overridden by server implementation
  }

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
}
