/**
 * WorkspaceStrategy - Unified workspace creation for coding tasks
 *
 * Abstracts workspace creation into a single interface:
 * - sandbox: Isolated directory for persona work (default)
 * - worktree: Git worktree on continuum repo with sparse checkout
 * - project: Git worktree on ANY external repo with full checkout + persona identity
 *
 * Each strategy creates a directory, registers it with the Rust backend
 * via CodeDaemon.createWorkspace(), and returns a handle + path.
 */

import { Commands } from '../../core/shared/Commands';
import { CodeDaemon } from '../../../daemons/code-daemon/shared/CodeDaemon';
import { Logger } from '../../core/logging/Logger';
import { SystemPaths } from '../../core/config/SystemPaths';
import { stringToUUID, type UUID } from '../../core/types/CrossPlatformUUID';
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

const log = Logger.create('WorkspaceStrategy', 'code');

// ────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────

export type WorkspaceMode = 'sandbox' | 'worktree' | 'project';

export interface WorkspaceConfig {
  /** Persona UUID (for handle generation + Rust backend registration) */
  readonly personaId: UUID;

  /** Human-readable persona identifier (e.g., 'together', 'deepseek') — used for ALL filesystem paths */
  readonly personaUniqueId: string;

  /** Which workspace strategy to use */
  readonly mode: WorkspaceMode;

  /** Short slug for branch naming (worktree/project mode): ai/{persona}/{slug} */
  readonly taskSlug?: string;

  /** Paths to sparse-checkout (worktree mode) */
  readonly sparsePaths?: string[];

  /** Absolute path to any git repo on disk (project mode) */
  readonly repoPath?: string;

  /** Persona display name for git identity (project mode) */
  readonly personaName?: string;
}

export interface WorkspaceResult {
  /** Handle to pass to code/* commands as userId */
  readonly handle: string;

  /** Absolute path to the workspace directory */
  readonly workspaceDir: string;

  /** Git branch name (worktree/project mode) */
  readonly branch?: string;

  /** Which mode was used */
  readonly mode: WorkspaceMode;

  /** Original repo path (project mode) — the repo the worktree was created from */
  readonly repoPath?: string;
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
    if (config.mode === 'project') {
      return this.createProject(config);
    }
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
   * Directory: {SystemPaths.personas.dir(uniqueId)}/workspace/
   * Registered with Rust backend as writable + read-only codebase access.
   */
  private static async createSandbox(config: WorkspaceConfig): Promise<WorkspaceResult> {
    const handle = config.personaId;
    const workspaceDir = path.join(SystemPaths.personas.dir(config.personaUniqueId), 'workspace');

    if (initializedWorkspaces.has(handle)) {
      return { handle, workspaceDir, mode: 'sandbox' };
    }

    // Create workspace directory if it doesn't exist
    if (!fs.existsSync(workspaceDir)) {
      fs.mkdirSync(workspaceDir, { recursive: true });
      log.info(`Created sandbox workspace: ${workspaceDir}`);
    }

    // Register with Rust backend — writable workspace + read-only codebase access
    // CRITICAL: Must register with personaId (UUID), not handle — Rust looks up by personaId
    const jtagRoot = process.cwd();
    await CodeDaemon.createWorkspace(config.personaId, workspaceDir, [jtagRoot]);
    initializedWorkspaces.add(handle);
    log.info(`Sandbox workspace initialized for persona ${config.personaUniqueId} (id=${config.personaId})`);

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
      // Already initialized — resolve path from convention (human-readable uniqueId)
      const jtagRoot = process.cwd();
      const workspaceDir = path.join(
        jtagRoot, '.continuum', 'sessions', 'user', 'shared', config.personaUniqueId, 'workspace',
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

    // Register with Rust backend — worktree is write location, main repo is read-only.
    // Worktrees with sparse checkout only contain the specified paths, not the full codebase.
    // Add main repo as read root so personas can explore the full codebase while writing to their worktree.
    // CRITICAL: Must register with personaId (UUID), not handle — Rust looks up by personaId
    const jtagRoot = process.cwd();
    await CodeDaemon.createWorkspace(config.personaId, workspaceDir, [jtagRoot]);
    initializedWorkspaces.add(handle);

    log.info(`Worktree workspace created: ${workspaceDir} (branch: ${branch}, personaId=${config.personaId})`);

    return { handle, workspaceDir, branch, mode: 'worktree' };
  }

  /**
   * Create a project workspace — git worktree on ANY external repo.
   *
   * Creates a branch per persona (ai/{personaName}/{slug}), sets local
   * git identity, and registers the worktree with the Rust CodeDaemon.
   * Supports working on any git-initialized directory on disk.
   */
  private static async createProject(config: WorkspaceConfig): Promise<WorkspaceResult> {
    if (!config.repoPath) {
      throw new Error('WorkspaceStrategy: project mode requires repoPath');
    }

    const slug = config.taskSlug ?? 'work';
    // Deterministic UUID handle from personaId + slug — strict UUID policy
    const handle = stringToUUID(`project:${config.personaId}:${slug}`);

    if (initializedWorkspaces.has(handle)) {
      // Already initialized — resolve from tracked data
      const meta = projectWorkspacePaths.get(handle);
      if (meta) {
        return { handle, workspaceDir: meta.worktreeDir, branch: meta.branch, mode: 'project', repoPath: config.repoPath };
      }
    }

    // Resolve repoPath — support relative paths from jtag root
    let resolvedRepoPath = path.isAbsolute(config.repoPath)
      ? config.repoPath
      : path.resolve(process.cwd(), config.repoPath);

    // Always resolve to the actual git root via rev-parse.
    // A .git directory or file at the provided path doesn't guarantee it's the repo root —
    // it could be a worktree .git file or a partial .git directory for local hooks.
    try {
      const gitRoot = execSync('git rev-parse --show-toplevel', {
        cwd: resolvedRepoPath,
        stdio: 'pipe',
      }).toString().trim();
      if (gitRoot !== resolvedRepoPath) {
        log.info(`Auto-detected git root: ${gitRoot} (from ${resolvedRepoPath})`);
        resolvedRepoPath = gitRoot;
      }
    } catch {
      throw new Error(`WorkspaceStrategy: not a git repo: ${resolvedRepoPath}`);
    }

    // Branch name: ai/{personaName}/{slug}
    const safeName = (config.personaName ?? config.personaUniqueId)
      .toLowerCase()
      .replace(/[^a-z0-9-]/g, '-')
      .replace(/-+/g, '-');
    const branchName = `ai/${safeName}/${slug}`;

    // Worktree directory: inside the repo's .git — uses uniqueId for human-readable paths
    const worktreeDir = path.join(resolvedRepoPath, '.git', 'continuum-worktrees', config.personaUniqueId, slug);

    log.info(`Creating project workspace: repo=${resolvedRepoPath} branch=${branchName}`);

    fs.mkdirSync(path.dirname(worktreeDir), { recursive: true });

    const gitOpts = { cwd: resolvedRepoPath, stdio: 'pipe' as const };

    // If worktree dir already exists from a previous session, reuse or force-remove it.
    if (fs.existsSync(worktreeDir)) {
      // Check if it's a valid git worktree checkout by looking for .git reference
      const gitRefFile = path.join(worktreeDir, '.git');
      if (fs.existsSync(gitRefFile)) {
        // Valid existing worktree — reuse it. Just ensure the branch is checked out.
        log.info(`Reusing existing worktree at ${worktreeDir} (branch: ${branchName})`);
      } else {
        // Stale directory without valid git reference — remove and recreate
        log.warn(`Removing stale worktree directory: ${worktreeDir}`);
        try { execSync(`git worktree remove "${worktreeDir}" --force`, gitOpts); } catch { /* ignore */ }
        try { execSync('git worktree prune', gitOpts); } catch { /* ignore */ }
        fs.rmSync(worktreeDir, { recursive: true, force: true });
      }
    }

    // Create worktree if directory doesn't exist (either first time or after cleanup)
    if (!fs.existsSync(worktreeDir)) {
      try {
        // Try creating with new branch from HEAD
        execSync(`git worktree add -b "${branchName}" "${worktreeDir}" HEAD`, gitOpts);
      } catch (e: any) {
        const errMsg = e.stderr?.toString() ?? e.message ?? '';
        if (errMsg.includes('already exists')) {
          // Branch exists but worktree dir was cleaned — checkout existing branch
          try {
            execSync(`git worktree add "${worktreeDir}" "${branchName}"`, gitOpts);
          } catch (e2: any) {
            const errMsg2 = e2.stderr?.toString() ?? e2.message ?? '';
            if (errMsg2.includes('already checked out')) {
              log.warn(`Branch ${branchName} checked out elsewhere — forcing worktree creation`);
              execSync(`git worktree add --force "${worktreeDir}" "${branchName}"`, gitOpts);
            } else {
              throw e2;
            }
          }
        } else {
          throw e;
        }
      }
    }

    // Set worktree-specific git identity (NOT main repo config!)
    // CRITICAL: Must use --worktree flag because worktrees share main repo's config.
    // Without --worktree, git config writes to the main repo's .git/config which pollutes
    // the human user's identity with AI persona info.
    const userName = config.personaName ?? 'AI Persona';
    const userEmail = `${config.personaUniqueId}@continuum.local`;
    const wtOpts = { cwd: worktreeDir, stdio: 'pipe' as const };

    // First enable worktree-specific config, then set the identity
    try {
      execSync('git config --worktree extensions.worktreeConfig true', wtOpts);
      execSync(`git config --worktree user.name "${userName}"`, wtOpts);
      execSync(`git config --worktree user.email "${userEmail}"`, wtOpts);
    } catch (configErr: any) {
      // Older git may not support --worktree; fall back to environment-based identity
      // which is safer than polluting the main repo's config
      log.warn(`Could not set worktree-specific config: ${configErr.message}. Commits will use GIT_AUTHOR_* env vars.`);
    }

    // Register with Rust CodeDaemon — worktree is the write location, JTAG root is read-only.
    // CRITICAL: Must register with personaId (UUID), not handle — Rust looks up by personaId
    // Use JTAG root (process.cwd()) as read root, NOT git root — code files are under src/
    const jtagRoot = process.cwd();
    log.info(`🔧 Registering workspace with Rust: personaId=${config.personaId}, workspaceRoot=${worktreeDir}, readRoots=[${jtagRoot}]`);
    await CodeDaemon.createWorkspace(config.personaId, worktreeDir, [jtagRoot]);
    log.info(`✅ Workspace registered with Rust for persona ${config.personaUniqueId}`);
    initializedWorkspaces.add(handle);
    projectWorkspacePaths.set(handle, { worktreeDir, branch: branchName, repoPath: resolvedRepoPath, personaId: config.personaId });
    personaToProjectHandle.set(config.personaId, handle);

    log.info(`Project workspace ready: ${worktreeDir} (handle: ${handle.slice(0, 8)}..., branch: ${branchName}, identity: ${userName} <${userEmail}>)`);

    return { handle, workspaceDir: worktreeDir, branch: branchName, mode: 'project', repoPath: resolvedRepoPath };
  }

  /**
   * Clean up a workspace.
   * - worktree-* handles: calls workspace/git/workspace/clean
   * - project-* handles: removes git worktree + optionally deletes branch
   * - other handles: skipped
   */
  static async cleanup(handle: string, options?: { force?: boolean; deleteBranch?: boolean }): Promise<void> {
    if (handle.startsWith('project-')) {
      return this.cleanupProject(handle, options);
    }

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

  /**
   * Clean up a project workspace — remove git worktree and optionally delete branch.
   */
  private static async cleanupProject(handle: string, options?: { force?: boolean; deleteBranch?: boolean }): Promise<void> {
    const meta = projectWorkspacePaths.get(handle);
    if (!meta) {
      log.warn(`No metadata for project handle ${handle}, removing from tracking`);
      initializedWorkspaces.delete(handle);
      return;
    }

    try {
      const gitOpts = { cwd: meta.repoPath, stdio: 'pipe' as const };

      // Remove the git worktree
      const forceFlag = options?.force ? ' --force' : '';
      execSync(`git worktree remove "${meta.worktreeDir}"${forceFlag}`, gitOpts);

      // Optionally delete the branch
      if (options?.deleteBranch && meta.branch) {
        try {
          execSync(`git branch -D "${meta.branch}"`, gitOpts);
          log.info(`Deleted branch ${meta.branch}`);
        } catch {
          log.warn(`Could not delete branch ${meta.branch} — may have upstream refs`);
        }
      }

      if (meta.personaId) {
        personaToProjectHandle.delete(meta.personaId);
      }
      initializedWorkspaces.delete(handle);
      projectWorkspacePaths.delete(handle);
      log.info(`Project workspace cleaned up: ${handle}`);
    } catch (error) {
      log.warn(`Project cleanup failed for ${handle}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * Get all active project workspace handles for a specific repo.
   * Used by RAG to discover team activity (who's working on what branch).
   */
  static getProjectHandlesForRepo(repoPath: string): Array<{ handle: string; branch: string; worktreeDir: string }> {
    const results: Array<{ handle: string; branch: string; worktreeDir: string }> = [];
    for (const [handle, meta] of projectWorkspacePaths) {
      if (meta.repoPath === repoPath) {
        results.push({ handle, branch: meta.branch, worktreeDir: meta.worktreeDir });
      }
    }
    return results;
  }

  /**
   * Get project workspace info for a specific persona.
   * Returns the first project workspace found (personas typically have one active project).
   * Used by ProjectContextSource (RAG) to inject project state.
   */
  static getProjectForPersona(personaId: string): ProjectWorkspaceMeta | undefined {
    const handle = personaToProjectHandle.get(personaId);
    if (handle) return projectWorkspacePaths.get(handle);
    return undefined;
  }

  /**
   * Get ALL project workspaces across all personas.
   * Used by ProjectContextSource to show team activity.
   */
  static get allProjectWorkspaces(): ReadonlyMap<string, ProjectWorkspaceMeta> {
    return projectWorkspacePaths;
  }
}

// ────────────────────────────────────────────────────────────
// Project workspace path tracking (needed for cleanup + team discovery)
// ────────────────────────────────────────────────────────────

interface ProjectWorkspaceMeta {
  readonly worktreeDir: string;
  readonly branch: string;
  readonly repoPath: string;
  readonly personaId: string;
}

const projectWorkspacePaths = new Map<string, ProjectWorkspaceMeta>();
/** Reverse index: personaId → handle (for RAG lookup) */
const personaToProjectHandle = new Map<string, string>();
