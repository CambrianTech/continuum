/**
 * WorkspaceStrategy - Unified workspace creation for coding tasks
 *
 * Abstracts the three workspace patterns into a single interface:
 * - sandbox: Isolated directory for persona work (default)
 * - worktree: Git worktree on real repo with sparse checkout
 * - challenge: Pre-seeded isolated workspace (handled by CodingChallengeRunner)
 *
 * Each strategy creates a directory, registers it with the Rust backend
 * via CodeDaemon.createWorkspace(), and returns a handle + path.
 */

import { Commands } from '../../core/shared/Commands';
import { CodeDaemon } from '../../../daemons/code-daemon/shared/CodeDaemon';
import { Logger } from '../../core/logging/Logger';
import * as fs from 'fs';
import * as path from 'path';

const log = Logger.create('WorkspaceStrategy', 'code');

// ────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────

export type WorkspaceMode = 'sandbox' | 'worktree';

export interface WorkspaceConfig {
  /** Persona ID creating the workspace */
  readonly personaId: string;

  /** Which workspace strategy to use */
  readonly mode: WorkspaceMode;

  /** Short slug for branch naming (worktree mode): ai/{persona}/{slug} */
  readonly taskSlug?: string;

  /** Paths to sparse-checkout (worktree mode) */
  readonly sparsePaths?: string[];
}

export interface WorkspaceResult {
  /** Handle to pass to code/* commands as userId */
  readonly handle: string;

  /** Absolute path to the workspace directory */
  readonly workspaceDir: string;

  /** Git branch name (worktree mode only) */
  readonly branch?: string;

  /** Which mode was used */
  readonly mode: WorkspaceMode;
}

// ────────────────────────────────────────────────────────────
// Track initialized workspaces to avoid re-creation
// ────────────────────────────────────────────────────────────

const initializedWorkspaces = new Set<string>();

// ────────────────────────────────────────────────────────────
// WorkspaceStrategy
// ────────────────────────────────────────────────────────────

export class WorkspaceStrategy {

  /**
   * Create a workspace for a coding task.
   *
   * @param config - Workspace configuration
   * @returns Handle, directory path, and optional branch name
   */
  static async create(config: WorkspaceConfig): Promise<WorkspaceResult> {
    if (config.mode === 'worktree') {
      return this.createWorktree(config);
    }
    return this.createSandbox(config);
  }

  /**
   * Check if a workspace has been initialized for the given handle.
   */
  static isInitialized(handle: string): boolean {
    return initializedWorkspaces.has(handle);
  }

  /**
   * Reset all tracked workspace handles.
   * Used by tests to ensure clean state between runs.
   */
  static resetTracking(): void {
    initializedWorkspaces.clear();
  }

  /**
   * Create an isolated sandbox workspace (current default behavior).
   * Directory: .continuum/personas/{personaId}/workspace/
   * Registered with Rust backend as writable + read-only codebase access.
   */
  private static async createSandbox(config: WorkspaceConfig): Promise<WorkspaceResult> {
    const handle = config.personaId;

    if (initializedWorkspaces.has(handle)) {
      const jtagRoot = process.cwd();
      const workspaceDir = path.join(jtagRoot, '.continuum', 'personas', config.personaId, 'workspace');
      return { handle, workspaceDir, mode: 'sandbox' };
    }

    const jtagRoot = process.cwd();
    const workspaceDir = path.join(jtagRoot, '.continuum', 'personas', config.personaId, 'workspace');

    // Create workspace directory if it doesn't exist
    if (!fs.existsSync(workspaceDir)) {
      fs.mkdirSync(workspaceDir, { recursive: true });
      log.info(`Created sandbox workspace: ${workspaceDir}`);
    }

    // Register with Rust backend — writable workspace + read-only codebase access
    await CodeDaemon.createWorkspace(handle, workspaceDir, [jtagRoot]);
    initializedWorkspaces.add(handle);
    log.info(`Sandbox workspace initialized for persona ${config.personaId}`);

    return { handle, workspaceDir, mode: 'sandbox' };
  }

  /**
   * Create a git worktree workspace for working on real repo source.
   * Uses workspace/git/workspace/init to create a sparse-checkout worktree,
   * then registers it with the Rust backend.
   */
  private static async createWorktree(config: WorkspaceConfig): Promise<WorkspaceResult> {
    const slug = config.taskSlug ?? 'work';
    const handle = `worktree-${config.personaId}-${slug}`;

    if (initializedWorkspaces.has(handle)) {
      // Already initialized — resolve path from convention
      const jtagRoot = process.cwd();
      const workspaceDir = path.join(
        jtagRoot, '.continuum', 'sessions', 'user', 'shared', config.personaId, 'workspace',
      );
      return { handle, workspaceDir, mode: 'worktree' };
    }

    if (!config.sparsePaths || config.sparsePaths.length === 0) {
      throw new Error('WorkspaceStrategy: worktree mode requires sparsePaths (which directories to checkout)');
    }

    log.info(`Creating worktree workspace for persona ${config.personaId} — paths: ${config.sparsePaths.join(', ')}`);

    // Call the existing workspace/git/workspace/init command
    const initResult = await Commands.execute<any, any>('workspace/git/workspace/init', {
      personaId: config.personaId,
      branch: `ai/${slug}`,
      paths: config.sparsePaths,
    });

    if (!initResult?.success) {
      throw new Error(`WorkspaceStrategy: worktree creation failed: ${initResult?.error?.message ?? 'Unknown error'}`);
    }

    const workspaceDir = initResult.workspacePath as string;
    const branch = initResult.branch as string;

    // Register with Rust backend — worktree IS the repo, no separate read roots needed
    // (the worktree contains the checked-out source files directly)
    await CodeDaemon.createWorkspace(handle, workspaceDir, []);
    initializedWorkspaces.add(handle);

    log.info(`Worktree workspace created: ${workspaceDir} (branch: ${branch})`);

    return { handle, workspaceDir, branch, mode: 'worktree' };
  }

  /**
   * Clean up a worktree workspace.
   * Calls workspace/git/workspace/clean and removes the handle from tracking.
   */
  static async cleanup(handle: string, options?: { force?: boolean; deleteBranch?: boolean }): Promise<void> {
    if (!handle.startsWith('worktree-')) {
      log.debug(`Skipping cleanup for non-worktree handle: ${handle}`);
      return;
    }

    try {
      await Commands.execute<any, any>('workspace/git/workspace/clean', {
        force: options?.force ?? false,
        deleteBranch: options?.deleteBranch ?? false,
      });
      initializedWorkspaces.delete(handle);
      log.info(`Worktree workspace cleaned up: ${handle}`);
    } catch (error) {
      log.warn(`Worktree cleanup failed for ${handle}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}
