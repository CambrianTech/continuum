/**
 * SentinelWorkspaceManager — Workspace lifecycle management for sentinel pipelines.
 *
 * Coordinates workspace creation and cleanup for sentinel CodingAgent steps.
 * Consumer of WorkspaceStrategy — adds sentinel-specific tracking so workspaces
 * are properly cleaned up when pipelines complete, fail, or are cancelled.
 *
 * Key invariant: (personaId, repoPath, taskSlug) → deterministic workspace.
 * Same triple always maps to the same worktree. Idempotent — re-running a
 * pipeline reuses the workspace, doesn't create a duplicate.
 */

import { WorkspaceStrategy, type WorkspaceResult } from '../code/server/WorkspaceStrategy';
import { Logger } from '../core/logging/Logger';
import type { UUID } from '../core/types/CrossPlatformUUID';

const log = Logger.create('SentinelWorkspaceManager', 'sentinel');

// ────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────

export interface SentinelWorkspaceConfig {
  /** Sentinel handle for tracking */
  readonly sentinelHandle: string;

  /** Persona UUID — the workspace registration key */
  readonly personaId: UUID;

  /** Human-readable persona identifier (e.g., 'helper', 'deepseek') */
  readonly personaUniqueId: string;

  /** Persona display name for git identity */
  readonly personaName: string;

  /** Absolute path to the git repo */
  readonly repoPath: string;

  /** Task slug for branch: ai/{persona}/{slug} */
  readonly taskSlug: string;

  /** Cleanup policy: 'keep' (default), 'on-success', 'always' */
  readonly cleanup?: 'keep' | 'on-success' | 'always';
}

interface TrackedWorkspace {
  readonly config: SentinelWorkspaceConfig;
  readonly workspace: WorkspaceResult;
  readonly registeredAt: number;
}

// ────────────────────────────────────────────────────────────
// SentinelWorkspaceManager
// ────────────────────────────────────────────────────────────

class SentinelWorkspaceManagerImpl {
  private _tracked = new Map<string, TrackedWorkspace>();

  /**
   * Acquire a workspace for a sentinel pipeline.
   *
   * Creates a project worktree via WorkspaceStrategy if one doesn't already exist.
   * The workspace is registered under personaId so code tools can find it.
   */
  async acquire(config: SentinelWorkspaceConfig): Promise<WorkspaceResult> {
    // Check if we already have a workspace for this sentinel
    const existing = this._tracked.get(config.sentinelHandle);
    if (existing) {
      log.info(`Reusing workspace for sentinel ${config.sentinelHandle}: ${existing.workspace.workspaceDir}`);
      return existing.workspace;
    }

    // Check if persona already has a workspace initialized
    if (WorkspaceStrategy.isInitialized(config.personaId)) {
      const project = WorkspaceStrategy.getProjectForPersona(config.personaId);
      if (project) {
        const result: WorkspaceResult = {
          handle: config.personaId,
          workspaceDir: project.worktreeDir,
          branch: project.branch,
          mode: 'project',
          repoPath: project.repoPath,
        };
        this._tracked.set(config.sentinelHandle, {
          config,
          workspace: result,
          registeredAt: Date.now(),
        });
        log.info(`Reusing existing persona workspace for sentinel ${config.sentinelHandle}: ${project.worktreeDir}`);
        return result;
      }
    }

    // Create a new project worktree
    log.info(`Creating workspace for sentinel ${config.sentinelHandle}: repo=${config.repoPath}, slug=${config.taskSlug}`);
    const workspace = await WorkspaceStrategy.create({
      personaId: config.personaId,
      personaUniqueId: config.personaUniqueId,
      mode: 'project',
      repoPath: config.repoPath,
      taskSlug: config.taskSlug,
      personaName: config.personaName,
    });

    this._tracked.set(config.sentinelHandle, {
      config,
      workspace,
      registeredAt: Date.now(),
    });

    log.info(`Workspace acquired for sentinel ${config.sentinelHandle}: ${workspace.workspaceDir} (branch: ${workspace.branch})`);
    return workspace;
  }

  /**
   * Release a workspace when a sentinel completes or fails.
   *
   * Cleanup behavior depends on the configured policy:
   * - 'keep': never clean up (default — user inspects results)
   * - 'on-success': clean up only if pipeline succeeded
   * - 'always': clean up regardless of outcome
   */
  async release(sentinelHandle: string, success: boolean): Promise<void> {
    const tracked = this._tracked.get(sentinelHandle);
    if (!tracked) {
      log.debug(`No tracked workspace for sentinel ${sentinelHandle}`);
      return;
    }

    const policy = tracked.config.cleanup ?? 'keep';
    const shouldCleanup = policy === 'always' || (policy === 'on-success' && success);

    if (shouldCleanup) {
      try {
        await WorkspaceStrategy.cleanup(tracked.workspace.handle, {
          force: false,
          deleteBranch: false,
        });
        log.info(`Workspace cleaned up for sentinel ${sentinelHandle} (policy=${policy}, success=${success})`);
      } catch (err) {
        log.warn(`Workspace cleanup failed for sentinel ${sentinelHandle}: ${err instanceof Error ? err.message : String(err)}`);
      }
    } else {
      log.info(`Workspace retained for sentinel ${sentinelHandle} (policy=${policy}, success=${success}): ${tracked.workspace.workspaceDir}`);
    }

    this._tracked.delete(sentinelHandle);
  }

  /**
   * Get workspace for a sentinel handle (if acquired).
   */
  get(sentinelHandle: string): WorkspaceResult | null {
    return this._tracked.get(sentinelHandle)?.workspace ?? null;
  }

  /**
   * List all active sentinel workspaces.
   */
  get active(): ReadonlyArray<{ handle: string; workspace: WorkspaceResult; registeredAt: number }> {
    return Array.from(this._tracked.values()).map(t => ({
      handle: t.config.sentinelHandle,
      workspace: t.workspace,
      registeredAt: t.registeredAt,
    }));
  }
}

/** Singleton instance */
export const SentinelWorkspaceManager = new SentinelWorkspaceManagerImpl();
