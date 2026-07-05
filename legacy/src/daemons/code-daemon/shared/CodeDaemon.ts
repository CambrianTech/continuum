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
  WorkspaceShellExecuteResponse,
  WorkspaceShellPollResponse,
  WorkspaceShellSessionInfo,
  WorkspaceShellWatchResponse,
  WorkspaceSentinelRule,
} from './CodeDaemonTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

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
  static async createWorkspace(personaId: UUID, workspaceRoot: string, readRoots?: string[]): Promise<void> {
    throw new Error('CodeDaemon.createWorkspace() must be implemented by server');
  }

  /**
   * Read a file from the persona's workspace.
   */
  static async workspaceRead(personaId: UUID, filePath: string, startLine?: number, endLine?: number): Promise<WorkspaceReadResult> {
    throw new Error('CodeDaemon.workspaceRead() must be implemented by server');
  }

  /**
   * Write or create a file in the persona's workspace.
   */
  static async workspaceWrite(personaId: UUID, filePath: string, content: string, description?: string): Promise<WorkspaceWriteResult> {
    throw new Error('CodeDaemon.workspaceWrite() must be implemented by server');
  }

  /**
   * Edit a file using one of four edit modes.
   */
  static async workspaceEdit(personaId: UUID, filePath: string, editMode: WorkspaceEditMode, description?: string): Promise<WorkspaceWriteResult> {
    throw new Error('CodeDaemon.workspaceEdit() must be implemented by server');
  }

  /**
   * Delete a file from the persona's workspace.
   */
  static async workspaceDelete(personaId: UUID, filePath: string, description?: string): Promise<WorkspaceWriteResult> {
    throw new Error('CodeDaemon.workspaceDelete() must be implemented by server');
  }

  /**
   * Preview an edit as a unified diff without applying it.
   */
  static async workspaceDiff(personaId: UUID, filePath: string, editMode: WorkspaceEditMode): Promise<{ success: boolean; unified: string }> {
    throw new Error('CodeDaemon.workspaceDiff() must be implemented by server');
  }

  /**
   * Undo a specific change or the last N changes.
   */
  static async workspaceUndo(personaId: UUID, changeId?: string, count?: number): Promise<WorkspaceUndoResult> {
    throw new Error('CodeDaemon.workspaceUndo() must be implemented by server');
  }

  /**
   * Get change history for a file or entire workspace.
   */
  static async workspaceHistory(personaId: UUID, filePath?: string, limit?: number): Promise<WorkspaceHistoryResult> {
    throw new Error('CodeDaemon.workspaceHistory() must be implemented by server');
  }

  /**
   * Search for a regex pattern across workspace files.
   */
  static async workspaceSearch(personaId: UUID, pattern: string, fileGlob?: string, maxResults?: number): Promise<WorkspaceSearchResult> {
    throw new Error('CodeDaemon.workspaceSearch() must be implemented by server');
  }

  /**
   * Generate a directory tree for the workspace.
   */
  static async workspaceTree(personaId: UUID, path?: string, maxDepth?: number, includeHidden?: boolean): Promise<WorkspaceTreeResult> {
    throw new Error('CodeDaemon.workspaceTree() must be implemented by server');
  }

  /**
   * Get git status for the workspace.
   */
  static async workspaceGitStatus(personaId: UUID): Promise<WorkspaceGitStatusInfo> {
    throw new Error('CodeDaemon.workspaceGitStatus() must be implemented by server');
  }

  /**
   * Get git diff for the workspace.
   */
  static async workspaceGitDiff(personaId: UUID, staged?: boolean): Promise<{ success: boolean; diff: string }> {
    throw new Error('CodeDaemon.workspaceGitDiff() must be implemented by server');
  }

  /**
   * Get git log for the workspace (last N commits).
   */
  static async workspaceGitLog(personaId: UUID, count?: number): Promise<{ success: boolean; log: string }> {
    throw new Error('CodeDaemon.workspaceGitLog() must be implemented by server');
  }

  /**
   * Stage files for commit in the workspace.
   */
  static async workspaceGitAdd(personaId: UUID, paths: string[]): Promise<{ staged: string[] }> {
    throw new Error('CodeDaemon.workspaceGitAdd() must be implemented by server');
  }

  /**
   * Create a git commit in the workspace.
   */
  static async workspaceGitCommit(personaId: UUID, message: string): Promise<{ hash: string }> {
    throw new Error('CodeDaemon.workspaceGitCommit() must be implemented by server');
  }

  /**
   * Push the workspace branch to remote.
   */
  static async workspaceGitPush(personaId: UUID, remote?: string, branch?: string): Promise<{ output: string }> {
    throw new Error('CodeDaemon.workspaceGitPush() must be implemented by server');
  }

  // ========================================================================
  // Shell Session Operations (Handle + Poll pattern)
  // ========================================================================

  /**
   * Create a shell session for a workspace.
   * The session persists cwd and env across command executions.
   */
  static async shellCreate(personaId: UUID, workspaceRoot: string): Promise<WorkspaceShellSessionInfo> {
    throw new Error('CodeDaemon.shellCreate() must be implemented by server');
  }

  /**
   * Execute a command in a shell session.
   *
   * Two modes:
   * - Handle mode (default): returns immediately with execution_id. Poll for output.
   * - Wait mode (wait=true): blocks until completion, returns full stdout/stderr.
   */
  static async shellExecute(
    personaId: UUID,
    cmd: string,
    options?: { timeoutMs?: number; wait?: boolean },
  ): Promise<WorkspaceShellExecuteResponse> {
    throw new Error('CodeDaemon.shellExecute() must be implemented by server');
  }

  /**
   * Poll an execution for new output since last poll.
   * Returns new stdout/stderr lines and status. Call until `finished` is true.
   */
  static async shellPoll(personaId: UUID, executionId: string): Promise<WorkspaceShellPollResponse> {
    throw new Error('CodeDaemon.shellPoll() must be implemented by server');
  }

  /**
   * Kill a running execution.
   */
  static async shellKill(personaId: UUID, executionId: string): Promise<void> {
    throw new Error('CodeDaemon.shellKill() must be implemented by server');
  }

  /**
   * Change shell session working directory (validated against workspace boundary).
   */
  static async shellCd(personaId: UUID, path: string): Promise<{ cwd: string }> {
    throw new Error('CodeDaemon.shellCd() must be implemented by server');
  }

  /**
   * Get shell session status/info.
   */
  static async shellStatus(personaId: UUID): Promise<WorkspaceShellSessionInfo> {
    throw new Error('CodeDaemon.shellStatus() must be implemented by server');
  }

  /**
   * Destroy shell session (kills all running executions).
   */
  static async shellDestroy(personaId: UUID): Promise<void> {
    throw new Error('CodeDaemon.shellDestroy() must be implemented by server');
  }

  // ========================================================================
  // Shell Watch + Sentinel (Event-driven output streaming)
  // ========================================================================

  /**
   * Watch a shell execution for new output.
   * Blocks until output is available — no timeout, no polling.
   * Returns classified output lines filtered through sentinel rules.
   * Call in a loop until `finished` is true.
   */
  static async shellWatch(personaId: UUID, executionId: string): Promise<WorkspaceShellWatchResponse> {
    throw new Error('CodeDaemon.shellWatch() must be implemented by server');
  }

  /**
   * Configure sentinel filter rules on a shell execution.
   * Rules classify output lines and control which are emitted or suppressed during watch.
   * Patterns are compiled to regex on the Rust side for performance.
   */
  static async shellSentinel(personaId: UUID, executionId: string, rules: WorkspaceSentinelRule[]): Promise<{ applied: boolean; ruleCount: number }> {
    throw new Error('CodeDaemon.shellSentinel() must be implemented by server');
  }
}
