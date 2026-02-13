/**
 * SentinelWorkspace - Git-based isolation for sentinel execution
 *
 * Sentinels should NEVER modify the user's working directory directly.
 * Instead, they work in isolated git branches/worktrees.
 *
 * ISOLATION MODES:
 *
 * 1. 'branch' (default) - Work on a temporary branch
 *    - Fast to create
 *    - Shares working directory (careful with uncommitted changes)
 *    - Good for: quick fixes, single sentinel
 *
 * 2. 'worktree' - Full filesystem isolation via git worktree
 *    - Complete isolation (separate checkout)
 *    - Can run builds without affecting main
 *    - Good for: parallel sentinels, long-running tasks
 *
 * 3. 'none' - Direct modification (DANGEROUS)
 *    - Only for testing or explicit user request
 *    - Sentinel edits user's actual files
 *
 * LIFECYCLE:
 *
 *   workspace = await SentinelWorkspace.create(config)
 *   workspace.workingDir  // Where sentinel should do work
 *
 *   ... sentinel runs ...
 *
 *   if (success) {
 *     await workspace.complete('merge')  // or 'pr' or 'leave'
 *   } else {
 *     await workspace.abort()  // Cleanup or leave for debugging
 *   }
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

export type IsolationMode = 'none' | 'branch' | 'worktree';
export type CompletionAction = 'merge' | 'pr' | 'leave';
export type AbortAction = 'delete' | 'leave';

export interface WorkspaceConfig {
  /** Original directory the sentinel was called from */
  callerDir: string;

  /** Isolation mode */
  isolation?: IsolationMode;

  /** Base branch to work from (default: current HEAD) */
  baseBranch?: string;

  /** Custom branch name (default: sentinel/{handle}) */
  branchName?: string;

  /** Unique handle for this sentinel run */
  handle: string;

  /** What to do on success */
  onSuccess?: CompletionAction;

  /** What to do on failure */
  onFailure?: AbortAction;

  /** Description for commits/PRs */
  description?: string;
}

export interface WorkspaceInfo {
  /** Where the sentinel should work */
  workingDir: string;

  /** The branch being used */
  branch: string;

  /** Original branch to return to */
  originalBranch: string;

  /** Whether this is a worktree */
  isWorktree: boolean;

  /** Worktree path (if isWorktree) */
  worktreePath?: string;
}

export class SentinelWorkspace {
  private config: Required<WorkspaceConfig>;
  private info: WorkspaceInfo | null = null;
  private completed = false;

  private constructor(config: WorkspaceConfig) {
    this.config = {
      isolation: 'branch',
      baseBranch: '',  // Will be detected
      branchName: `sentinel/${config.handle}`,
      onSuccess: 'leave',
      onFailure: 'leave',
      description: `Sentinel ${config.handle}`,
      ...config,
    };
  }

  /**
   * Create and initialize a workspace
   */
  static async create(config: WorkspaceConfig): Promise<SentinelWorkspace> {
    const workspace = new SentinelWorkspace(config);
    await workspace.initialize();
    return workspace;
  }

  /**
   * The directory where the sentinel should do its work
   */
  get workingDir(): string {
    if (!this.info) throw new Error('Workspace not initialized');
    return this.info.workingDir;
  }

  /**
   * Get full workspace info
   */
  get workspace(): WorkspaceInfo {
    if (!this.info) throw new Error('Workspace not initialized');
    return this.info;
  }

  /**
   * Initialize the workspace
   */
  private async initialize(): Promise<void> {
    const { callerDir, isolation, branchName } = this.config;

    // Check if callerDir is a git repo
    const isGitRepo = this.isGitRepository(callerDir);

    if (isolation === 'none' || !isGitRepo) {
      // No isolation - work directly in callerDir
      this.info = {
        workingDir: callerDir,
        branch: isGitRepo ? this.getCurrentBranch(callerDir) : 'none',
        originalBranch: isGitRepo ? this.getCurrentBranch(callerDir) : 'none',
        isWorktree: false,
      };
      return;
    }

    // Get current branch
    const originalBranch = this.getCurrentBranch(callerDir);
    const baseBranch = this.config.baseBranch || originalBranch;

    if (isolation === 'branch') {
      // Create and switch to a new branch
      await this.createBranch(callerDir, branchName, baseBranch);
      this.info = {
        workingDir: callerDir,
        branch: branchName,
        originalBranch,
        isWorktree: false,
      };
    } else if (isolation === 'worktree') {
      // Create a git worktree for full isolation
      const worktreePath = await this.createWorktree(callerDir, branchName, baseBranch);
      this.info = {
        workingDir: worktreePath,
        branch: branchName,
        originalBranch,
        isWorktree: true,
        worktreePath,
      };
    }
  }

  /**
   * Complete the workspace (on success)
   */
  async complete(action?: CompletionAction): Promise<{ merged?: boolean; prUrl?: string; branch: string }> {
    if (!this.info) throw new Error('Workspace not initialized');
    if (this.completed) throw new Error('Workspace already completed');
    this.completed = true;

    const finalAction = action || this.config.onSuccess;
    const { workingDir, branch, originalBranch, isWorktree, worktreePath } = this.info;

    // Commit any uncommitted changes
    if (this.hasUncommittedChanges(workingDir)) {
      this.commitChanges(workingDir, `Sentinel work: ${this.config.description}`);
    }

    let result: { merged?: boolean; prUrl?: string; branch: string } = { branch };

    if (finalAction === 'merge') {
      // Switch back and merge
      if (isWorktree) {
        // Merge from worktree branch into original
        execSync(`git merge ${branch} --no-edit`, { cwd: this.config.callerDir });
        this.cleanupWorktree(worktreePath!, branch);
      } else {
        execSync(`git checkout ${originalBranch}`, { cwd: workingDir });
        execSync(`git merge ${branch} --no-edit`, { cwd: workingDir });
        execSync(`git branch -d ${branch}`, { cwd: workingDir });
      }
      result.merged = true;
    } else if (finalAction === 'pr') {
      // Push and create PR (requires gh CLI)
      execSync(`git push -u origin ${branch}`, { cwd: isWorktree ? worktreePath! : workingDir });
      try {
        const prOutput = execSync(
          `gh pr create --title "${this.config.description}" --body "Automated by Sentinel ${this.config.handle}" --head ${branch}`,
          { cwd: isWorktree ? worktreePath! : workingDir, encoding: 'utf-8' }
        );
        result.prUrl = prOutput.trim();
      } catch {
        // gh CLI might not be available
        console.warn('Could not create PR via gh CLI');
      }

      // Return to original branch if not worktree
      if (!isWorktree) {
        execSync(`git checkout ${originalBranch}`, { cwd: workingDir });
      } else {
        this.cleanupWorktree(worktreePath!, branch);
      }
    } else {
      // 'leave' - just switch back, leave branch for review
      if (!isWorktree) {
        execSync(`git checkout ${originalBranch}`, { cwd: workingDir });
      } else {
        this.cleanupWorktree(worktreePath!, branch);
      }
    }

    return result;
  }

  /**
   * Abort the workspace (on failure)
   */
  async abort(action?: AbortAction): Promise<void> {
    if (!this.info) throw new Error('Workspace not initialized');
    if (this.completed) throw new Error('Workspace already completed');
    this.completed = true;

    const finalAction = action || this.config.onFailure;
    const { workingDir, branch, originalBranch, isWorktree, worktreePath } = this.info;

    if (finalAction === 'delete') {
      // Discard all changes and delete branch
      if (isWorktree) {
        this.cleanupWorktree(worktreePath!, branch);
        execSync(`git branch -D ${branch}`, { cwd: this.config.callerDir, stdio: 'ignore' });
      } else {
        // Reset all uncommitted changes (including untracked files)
        execSync('git reset --hard HEAD', { cwd: workingDir, stdio: 'ignore' });
        execSync('git clean -fd', { cwd: workingDir, stdio: 'ignore' });  // Remove untracked files
        execSync(`git checkout ${originalBranch}`, { cwd: workingDir });
        execSync(`git branch -D ${branch}`, { cwd: workingDir, stdio: 'ignore' });
      }
    } else {
      // 'leave' - keep branch for debugging
      if (!isWorktree) {
        execSync(`git checkout ${originalBranch}`, { cwd: workingDir });
      } else {
        // Leave worktree for inspection but note in console
        console.log(`Sentinel worktree left for debugging: ${worktreePath}`);
      }
    }
  }

  /**
   * Get changes made in this workspace
   */
  getChanges(): { files: string[]; diff: string } {
    if (!this.info) throw new Error('Workspace not initialized');
    const { workingDir } = this.info;

    const files = execSync('git diff --name-only HEAD~1 2>/dev/null || git diff --name-only', {
      cwd: workingDir,
      encoding: 'utf-8',
    }).trim().split('\n').filter(f => f);

    const diff = execSync('git diff HEAD~1 2>/dev/null || git diff', {
      cwd: workingDir,
      encoding: 'utf-8',
    });

    return { files, diff };
  }

  // --- Git helpers ---

  private isGitRepository(dir: string): boolean {
    try {
      execSync('git rev-parse --git-dir', { cwd: dir, stdio: 'ignore' });
      return true;
    } catch {
      return false;
    }
  }

  private getCurrentBranch(dir: string): string {
    return execSync('git branch --show-current', { cwd: dir, encoding: 'utf-8' }).trim();
  }

  private createBranch(dir: string, branchName: string, baseBranch: string): void {
    // Stash any uncommitted changes
    const hasChanges = this.hasUncommittedChanges(dir);
    if (hasChanges) {
      execSync('git stash', { cwd: dir });
    }

    // Create and checkout new branch
    try {
      execSync(`git checkout -b ${branchName} ${baseBranch}`, { cwd: dir });
    } catch {
      // Branch might already exist, try to checkout
      execSync(`git checkout ${branchName}`, { cwd: dir });
    }

    // Restore stashed changes
    if (hasChanges) {
      try {
        execSync('git stash pop', { cwd: dir });
      } catch {
        // Stash might have conflicts, leave it
      }
    }
  }

  private createWorktree(dir: string, branchName: string, baseBranch: string): string {
    // Create worktree in a predictable location
    const gitRoot = execSync('git rev-parse --show-toplevel', { cwd: dir, encoding: 'utf-8' }).trim();
    const worktreePath = path.join(gitRoot, '.sentinel-workspaces', this.config.handle);

    // Ensure parent directory exists
    fs.mkdirSync(path.dirname(worktreePath), { recursive: true });

    // Create worktree with new branch
    execSync(`git worktree add -b ${branchName} "${worktreePath}" ${baseBranch}`, { cwd: dir });

    return worktreePath;
  }

  private cleanupWorktree(worktreePath: string, branchName: string): void {
    try {
      execSync(`git worktree remove "${worktreePath}" --force`, { cwd: this.config.callerDir });
    } catch {
      // Manual cleanup if git worktree remove fails
      fs.rmSync(worktreePath, { recursive: true, force: true });
      execSync(`git worktree prune`, { cwd: this.config.callerDir });
    }
  }

  private hasUncommittedChanges(dir: string): boolean {
    const status = execSync('git status --porcelain', { cwd: dir, encoding: 'utf-8' });
    return status.trim().length > 0;
  }

  private commitChanges(dir: string, message: string): void {
    execSync('git add -A', { cwd: dir });
    execSync(`git commit -m "${message}"`, { cwd: dir });
  }
}

/**
 * Quick helper to run a sentinel with workspace isolation
 */
export async function withSentinelWorkspace<T>(
  config: Omit<WorkspaceConfig, 'handle'> & { handle?: string },
  fn: (workingDir: string, workspace: SentinelWorkspace) => Promise<T>
): Promise<{ result: T; workspace: WorkspaceInfo }> {
  const handle = config.handle || `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  const workspace = await SentinelWorkspace.create({ ...config, handle });

  try {
    const result = await fn(workspace.workingDir, workspace);
    await workspace.complete();
    return { result, workspace: workspace.workspace };
  } catch (error) {
    await workspace.abort();
    throw error;
  }
}
