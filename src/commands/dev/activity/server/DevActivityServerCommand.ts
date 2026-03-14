/**
 * dev/activity — Show what each persona and sentinel has edited.
 *
 * Queries:
 * 1. WorkspaceStrategy for active project workspaces (persona → worktree mapping)
 * 2. Git status + recent commits for each workspace
 * 3. Rust sentinel handles for active/recent sentinels
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type {
  DevActivityParams,
  DevActivityResult,
  WorkspaceActivity,
  SentinelActivity,
  CommitInfo,
} from '../shared/DevActivityTypes';
import { WorkspaceStrategy } from '../../../../system/code/server/WorkspaceStrategy';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { promisify } from 'util';
import { exec } from 'child_process';
import * as fs from 'fs';

const execAsync = promisify(exec);

export class DevActivityServerCommand extends CommandBase<DevActivityParams, DevActivityResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('dev/activity', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<DevActivityResult> {
    const typed = params as JTAGPayload & DevActivityParams;
    const limit = typed.limit || 10;

    const workspaces: WorkspaceActivity[] = [];
    const sentinels: SentinelActivity[] = [];

    // 1. Workspace activity (skip if sentinelsOnly)
    if (!typed.sentinelsOnly) {
      const allWorkspaces = WorkspaceStrategy.allProjectWorkspaces;

      for (const [, meta] of allWorkspaces) {
        // Filter by personaId if specified
        if (typed.personaId && meta.personaId !== typed.personaId) continue;
        // Filter by repo if specified
        if (typed.repo && meta.repoPath !== typed.repo) continue;

        if (!fs.existsSync(meta.worktreeDir)) continue;

        const activity = await this.getWorkspaceActivity(meta);
        if (activity) workspaces.push(activity);
      }
    }

    // 2. Sentinel activity
    try {
      const client = RustCoreIPCClient.getInstance();
      const listResult = await client.sentinelList();

      for (const handle of (listResult.handles || []).slice(0, limit)) {
        // Filter by personaId if we could (sentinels don't store this in Rust handle)
        sentinels.push({
          handle: handle.id,
          name: handle.id.slice(0, 12),
          status: handle.status,
          workingDir: handle.workingDir,
          startedAt: new Date(handle.startTime).toISOString(),
          durationMs: handle.endTime
            ? handle.endTime - handle.startTime
            : Date.now() - handle.startTime,
          error: handle.error ?? undefined,
        });
      }
    } catch (err) {
      console.warn(`[DevActivity] Could not query sentinels: ${err}`);
    }

    // Sort sentinels: running first, then by start time desc
    sentinels.sort((a, b) => {
      if (a.status === 'running' && b.status !== 'running') return -1;
      if (b.status === 'running' && a.status !== 'running') return 1;
      return (b.startedAt || '').localeCompare(a.startedAt || '');
    });

    return transformPayload(params, {
      success: true,
      workspaces,
      sentinels: sentinels.slice(0, limit),
    });
  }

  private async getWorkspaceActivity(
    meta: { worktreeDir: string; branch: string; repoPath: string; personaId: string },
  ): Promise<WorkspaceActivity | null> {
    const cwd = meta.worktreeDir;
    const opts = { cwd, timeout: 5000 };

    try {
      // Git identity (persona name from worktree config)
      let personaName = 'Unknown';
      try {
        const { stdout } = await execAsync('git config user.name', opts);
        personaName = stdout.trim() || 'Unknown';
      } catch { /* use default */ }

      // Modified files
      let modified: string[] = [];
      try {
        const { stdout } = await execAsync('git diff --name-only', opts);
        modified = stdout.trim().split('\n').filter(f => f);
      } catch { /* empty */ }

      // Staged files
      let staged: string[] = [];
      try {
        const { stdout } = await execAsync('git diff --cached --name-only', opts);
        staged = stdout.trim().split('\n').filter(f => f);
      } catch { /* empty */ }

      // Untracked files
      let untracked: string[] = [];
      try {
        const { stdout } = await execAsync('git ls-files --others --exclude-standard', opts);
        untracked = stdout.trim().split('\n').filter(f => f);
      } catch { /* empty */ }

      // Recent commits on this branch (up to 10)
      const recentCommits: CommitInfo[] = [];
      try {
        const { stdout } = await execAsync(
          'git log --oneline --format="%h|%s|%ci" -10',
          opts,
        );
        for (const line of stdout.trim().split('\n').filter(l => l)) {
          const [hash, message, date] = line.split('|');
          if (hash) recentCommits.push({ hash, message: message || '', date: date || '' });
        }
      } catch { /* empty — maybe no commits yet */ }

      return {
        personaId: meta.personaId,
        personaName,
        branch: meta.branch,
        worktreeDir: meta.worktreeDir,
        repoPath: meta.repoPath,
        modified,
        staged,
        untracked,
        recentCommits,
      };
    } catch {
      return null;
    }
  }
}
