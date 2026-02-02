/**
 * Workspace - Bound handle for all workspace-scoped code operations
 *
 * Instead of passing userId/handle to every CodeDaemon call,
 * create a Workspace instance and use it directly:
 *
 *   const ws = await Workspace.create({ personaId, mode: 'worktree', taskSlug: 'fix-auth' });
 *   const tree = await ws.tree();
 *   const file = await ws.read('src/auth.ts');
 *   await ws.edit('src/auth.ts', { editType: 'search_replace', search: 'old', replace: 'new' });
 *   const result = await ws.verify(true);
 *   if (!result.success) { // read errors, fix, verify again }
 *   await ws.gitAdd(['.']);
 *   await ws.gitCommit('Fix auth token validation');
 *   await ws.destroy();
 */

import { CodeDaemon } from '../../../daemons/code-daemon/shared/CodeDaemon';
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
  WorkspaceClassifiedLine,
} from '../../../daemons/code-daemon/shared/CodeDaemonTypes';
import { WorkspaceStrategy } from './WorkspaceStrategy';
import type { WorkspaceMode, WorkspaceConfig } from './WorkspaceStrategy';
import { CodeVerify, type CodeVerifyResult } from '../../../commands/code/verify/shared/CodeVerifyTypes';

export class Workspace {

  private constructor(
    /** Handle string used to identify this workspace in the Rust backend */
    readonly handle: string,
    /** Absolute path to the workspace directory on disk */
    readonly dir: string,
    /** Whether this is a sandbox or git worktree workspace */
    readonly mode: WorkspaceMode,
    /** Git branch name (worktree mode only) */
    readonly branch?: string,
  ) {}

  /**
   * Create and register a new workspace.
   * Returns a bound handle that encapsulates all workspace operations.
   */
  static async create(config: WorkspaceConfig): Promise<Workspace> {
    const result = await WorkspaceStrategy.create(config);
    return new Workspace(result.handle, result.workspaceDir, result.mode, result.branch);
  }

  /**
   * Create a Workspace from an already-initialized handle.
   * Useful when resuming a workspace that was previously created.
   */
  static fromExisting(handle: string, dir: string, mode: WorkspaceMode, branch?: string): Workspace {
    return new Workspace(handle, dir, mode, branch);
  }

  // ════════════════════════════════════════════════════════════
  // File Operations
  // ════════════════════════════════════════════════════════════

  /** Read a file from this workspace */
  async read(filePath: string, startLine?: number, endLine?: number): Promise<WorkspaceReadResult> {
    return CodeDaemon.workspaceRead(this.handle, filePath, startLine, endLine);
  }

  /** Write or create a file in this workspace */
  async write(filePath: string, content: string, description?: string): Promise<WorkspaceWriteResult> {
    return CodeDaemon.workspaceWrite(this.handle, filePath, content, description);
  }

  /** Edit a file using one of four edit modes */
  async edit(filePath: string, editMode: WorkspaceEditMode, description?: string): Promise<WorkspaceWriteResult> {
    return CodeDaemon.workspaceEdit(this.handle, filePath, editMode, description);
  }

  /** Delete a file from this workspace */
  async delete(filePath: string, description?: string): Promise<WorkspaceWriteResult> {
    return CodeDaemon.workspaceDelete(this.handle, filePath, description);
  }

  /** Preview an edit as unified diff without applying */
  async diff(filePath: string, editMode: WorkspaceEditMode): Promise<{ success: boolean; unified: string }> {
    return CodeDaemon.workspaceDiff(this.handle, filePath, editMode);
  }

  // ════════════════════════════════════════════════════════════
  // Search & Discovery
  // ════════════════════════════════════════════════════════════

  /** Search for a regex pattern across workspace files */
  async search(pattern: string, fileGlob?: string, maxResults?: number): Promise<WorkspaceSearchResult> {
    return CodeDaemon.workspaceSearch(this.handle, pattern, fileGlob, maxResults);
  }

  /** Get directory tree structure */
  async tree(path?: string, maxDepth?: number, includeHidden?: boolean): Promise<WorkspaceTreeResult> {
    return CodeDaemon.workspaceTree(this.handle, path, maxDepth, includeHidden);
  }

  // ════════════════════════════════════════════════════════════
  // Change Tracking
  // ════════════════════════════════════════════════════════════

  /** Undo a specific change or the last N changes */
  async undo(changeId?: string, count?: number): Promise<WorkspaceUndoResult> {
    return CodeDaemon.workspaceUndo(this.handle, changeId, count);
  }

  /** Get change history for a file or entire workspace */
  async history(filePath?: string, limit?: number): Promise<WorkspaceHistoryResult> {
    return CodeDaemon.workspaceHistory(this.handle, filePath, limit);
  }

  // ════════════════════════════════════════════════════════════
  // Verification
  // ════════════════════════════════════════════════════════════

  /** Run TypeScript compilation check and optionally tests */
  async verify(typeCheck?: boolean, testFiles?: string[]): Promise<CodeVerifyResult> {
    return CodeVerify.execute({
      userId: this.handle,
      typeCheck,
      testFiles,
    });
  }

  // ════════════════════════════════════════════════════════════
  // Git Operations
  // ════════════════════════════════════════════════════════════

  /** Get git status for this workspace */
  async gitStatus(): Promise<WorkspaceGitStatusInfo> {
    return CodeDaemon.workspaceGitStatus(this.handle);
  }

  /** Get git diff (staged or unstaged) */
  async gitDiff(staged?: boolean): Promise<{ success: boolean; diff: string }> {
    return CodeDaemon.workspaceGitDiff(this.handle, staged);
  }

  /** Get git log (last N commits) */
  async gitLog(count?: number): Promise<{ success: boolean; log: string }> {
    return CodeDaemon.workspaceGitLog(this.handle, count);
  }

  /** Stage files for commit */
  async gitAdd(paths: string[]): Promise<{ staged: string[] }> {
    return CodeDaemon.workspaceGitAdd(this.handle, paths);
  }

  /** Create a git commit */
  async gitCommit(message: string): Promise<{ hash: string }> {
    return CodeDaemon.workspaceGitCommit(this.handle, message);
  }

  /** Push the workspace branch to remote */
  async gitPush(remote?: string, branch?: string): Promise<{ output: string }> {
    return CodeDaemon.workspaceGitPush(this.handle, remote, branch);
  }

  // ════════════════════════════════════════════════════════════
  // Shell Session (Rust-backed, persistent per workspace)
  // ════════════════════════════════════════════════════════════

  private _shellCreated = false;

  /**
   * Ensure the Rust-side shell session exists for this workspace.
   * Called automatically by shell methods — idempotent after first call.
   */
  private async ensureShell(): Promise<void> {
    if (this._shellCreated) return;
    await CodeDaemon.shellCreate(this.handle, this.dir);
    this._shellCreated = true;
  }

  /**
   * Execute a shell command synchronously (blocks until completion).
   * Use for quick commands: `git status`, `npm test`, `ls`.
   *
   * The shell session retains cwd and env across calls — just like
   * a real terminal. First call auto-creates the session.
   */
  async exec(cmd: string, timeoutMs?: number): Promise<WorkspaceShellExecuteResponse> {
    await this.ensureShell();
    return CodeDaemon.shellExecute(this.handle, cmd, {
      timeoutMs: timeoutMs ?? 30000,
      wait: true,
    });
  }

  /**
   * Execute a shell command asynchronously (returns handle immediately).
   * Use for long-running commands: `cargo build`, `npm run build`.
   *
   * Returns an execution_id. Call shellPoll() to stream output,
   * shellKill() to abort.
   */
  async execAsync(cmd: string, timeoutMs?: number): Promise<WorkspaceShellExecuteResponse> {
    await this.ensureShell();
    return CodeDaemon.shellExecute(this.handle, cmd, {
      timeoutMs,
      wait: false,
    });
  }

  /** Poll a running execution for new stdout/stderr since last poll */
  async shellPoll(executionId: string): Promise<WorkspaceShellPollResponse> {
    return CodeDaemon.shellPoll(this.handle, executionId);
  }

  /** Kill a running execution */
  async shellKill(executionId: string): Promise<void> {
    return CodeDaemon.shellKill(this.handle, executionId);
  }

  /** Change the shell session's working directory */
  async shellCd(path: string): Promise<{ cwd: string }> {
    await this.ensureShell();
    return CodeDaemon.shellCd(this.handle, path);
  }

  /** Get shell session info (cwd, env, running executions) */
  async shellStatus(): Promise<WorkspaceShellSessionInfo> {
    await this.ensureShell();
    return CodeDaemon.shellStatus(this.handle);
  }

  // ════════════════════════════════════════════════════════════
  // Shell Watch + Sentinel (Event-driven output streaming)
  // ════════════════════════════════════════════════════════════

  /**
   * Configure sentinel filter rules on a running execution.
   * Rules classify output lines and control which are emitted or suppressed during watch().
   * Patterns are compiled to regex on the Rust side for performance.
   */
  async sentinel(executionId: string, rules: WorkspaceSentinelRule[]): Promise<{ applied: boolean; ruleCount: number }> {
    return CodeDaemon.shellSentinel(this.handle, executionId, rules);
  }

  /**
   * Watch a running execution for new output.
   * Blocks until output is available — no timeout, no polling.
   * Returns classified lines filtered through sentinel rules.
   * Call in a loop until `finished` is true.
   */
  async watch(executionId: string): Promise<WorkspaceShellWatchResponse> {
    await this.ensureShell();
    return CodeDaemon.shellWatch(this.handle, executionId);
  }

  /**
   * Execute a command and watch its output with optional sentinel filtering.
   * Convenience composition: exec → sentinel → watch loop.
   *
   * @param cmd Command to execute
   * @param rules Optional sentinel filter rules
   * @param onLine Optional callback for each classified line
   * @returns Final watch response (finished=true, has exit_code)
   */
  async execWatch(
    cmd: string,
    rules?: WorkspaceSentinelRule[],
    onLine?: (line: WorkspaceClassifiedLine) => void,
  ): Promise<WorkspaceShellWatchResponse> {
    const exec = await this.execAsync(cmd);

    if (rules?.length) {
      await this.sentinel(exec.execution_id, rules);
    }

    let response: WorkspaceShellWatchResponse;
    do {
      response = await this.watch(exec.execution_id);
      if (onLine) {
        for (const line of response.lines) {
          onLine(line);
        }
      }
    } while (!response.finished);

    return response;
  }

  // ════════════════════════════════════════════════════════════
  // Lifecycle
  // ════════════════════════════════════════════════════════════

  /** Clean up this workspace (shell session + worktree removal + handle deregistration) */
  async destroy(options?: { force?: boolean; deleteBranch?: boolean }): Promise<void> {
    if (this._shellCreated) {
      await CodeDaemon.shellDestroy(this.handle);
      this._shellCreated = false;
    }
    await WorkspaceStrategy.cleanup(this.handle, options);
  }
}
