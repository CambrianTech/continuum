/**
 * Workspace List Command - Server Implementation
 *
 * Discovers ALL persona workspaces by scanning:
 * 1. On-disk git worktrees at .git/continuum-worktrees/{personaId}/{slug}/
 * 2. In-memory active workspaces from WorkspaceStrategy
 *
 * For each discovered workspace, optionally queries git status (branch,
 * modified files, staged files, commits ahead, HEAD info).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { WorkspaceListParams, WorkspaceListResult, WorkspaceInfo, WorkspaceGitStatus } from '../shared/WorkspaceListTypes';
import { createWorkspaceListResultFromParams } from '../shared/WorkspaceListTypes';
import { WorkspaceStrategy } from '../../../../system/code/server/WorkspaceStrategy';
import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

export class WorkspaceListServerCommand extends CommandBase<WorkspaceListParams, WorkspaceListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('workspace/list', context, subpath, commander);
  }

  async execute(params: WorkspaceListParams): Promise<WorkspaceListResult> {
    const includeGitStatus = params.includeGitStatus !== false; // default true
    const filterPersona = params.personaId || undefined;

    // Phase 1: Scan on-disk worktrees
    const diskWorkspaces = this.scanDiskWorktrees(filterPersona);

    // Phase 2: Merge in-memory active state
    const activeHandles = new Set<string>();
    for (const [_handle, meta] of WorkspaceStrategy.allProjectWorkspaces) {
      activeHandles.add(meta.worktreeDir);
    }

    // Mark workspaces as active if they're in the in-memory map
    for (const ws of diskWorkspaces) {
      ws.active = activeHandles.has(ws.worktreeDir);
    }

    // Phase 3: Add any in-memory workspaces not found on disk (sandbox mode, etc.)
    for (const [_handle, meta] of WorkspaceStrategy.allProjectWorkspaces) {
      const alreadyListed = diskWorkspaces.some(w => w.worktreeDir === meta.worktreeDir);
      if (!alreadyListed) {
        // Extract personaId from worktreeDir path convention:
        // .git/continuum-worktrees/{personaId}/{slug}
        const parts = meta.worktreeDir.split(path.sep);
        const worktreeIdx = parts.indexOf('continuum-worktrees');
        const personaId = worktreeIdx >= 0 ? parts[worktreeIdx + 1] : 'unknown';
        const slug = worktreeIdx >= 0 ? parts[worktreeIdx + 2] : 'unknown';

        if (filterPersona && personaId !== filterPersona) continue;

        diskWorkspaces.push({
          personaId,
          taskSlug: slug,
          worktreeDir: meta.worktreeDir,
          branch: meta.branch,
          active: true,
          mode: 'project',
        });
      }
    }

    // Phase 4: Optionally get git status for each workspace
    if (includeGitStatus) {
      const statusPromises = diskWorkspaces.map(ws => this.getGitStatus(ws));
      await Promise.all(statusPromises);
    }

    // Sort: active first, then by persona name
    diskWorkspaces.sort((a, b) => {
      if (a.active !== b.active) return a.active ? -1 : 1;
      return a.personaId.localeCompare(b.personaId);
    });

    const activeCount = diskWorkspaces.filter(w => w.active).length;

    return createWorkspaceListResultFromParams(params, {
      success: true,
      workspaces: diskWorkspaces,
      totalCount: diskWorkspaces.length,
      activeCount,
    });
  }

  /**
   * Scan .git/continuum-worktrees/ for on-disk worktrees.
   * Directory convention: .git/continuum-worktrees/{personaUniqueId}/{taskSlug}/
   */
  private scanDiskWorktrees(filterPersona?: string): WorkspaceInfo[] {
    const worktreeRoot = path.resolve(process.cwd(), '..', '..', '..', '.git', 'continuum-worktrees');

    // Also check the main repo .git/continuum-worktrees (resolve from git root)
    let gitRoot: string;
    try {
      gitRoot = execSync('git rev-parse --show-toplevel', {
        cwd: process.cwd(),
        stdio: 'pipe',
        timeout: 3000,
      }).toString().trim();
    } catch {
      gitRoot = process.cwd();
    }

    const worktreeDirs = [
      path.join(gitRoot, '.git', 'continuum-worktrees'),
    ];

    const workspaces: WorkspaceInfo[] = [];

    for (const baseDir of worktreeDirs) {
      if (!fs.existsSync(baseDir)) continue;

      let personaDirs: string[];
      try {
        personaDirs = fs.readdirSync(baseDir).filter(entry => {
          const entryPath = path.join(baseDir, entry);
          return fs.statSync(entryPath).isDirectory();
        });
      } catch {
        continue;
      }

      for (const personaDir of personaDirs) {
        if (filterPersona && personaDir !== filterPersona) continue;

        const personaPath = path.join(baseDir, personaDir);
        let slugDirs: string[];
        try {
          slugDirs = fs.readdirSync(personaPath).filter(entry => {
            const entryPath = path.join(personaPath, entry);
            return fs.statSync(entryPath).isDirectory();
          });
        } catch {
          continue;
        }

        for (const slug of slugDirs) {
          const worktreeDir = path.join(personaPath, slug);

          // Validate it's actually a git worktree (has a .git file/dir)
          const gitRef = path.join(worktreeDir, '.git');
          if (!fs.existsSync(gitRef)) continue;

          // Read branch from the worktree
          let branch = '';
          try {
            branch = execSync('git branch --show-current', {
              cwd: worktreeDir,
              stdio: 'pipe',
              timeout: 3000,
            }).toString().trim();
          } catch {
            branch = 'detached';
          }

          workspaces.push({
            personaId: personaDir,
            taskSlug: slug,
            worktreeDir,
            branch,
            active: false, // Will be set in phase 2
            mode: 'project',
          });
        }
      }
    }

    return workspaces;
  }

  /**
   * Populate git status on a workspace info object (mutates in place).
   */
  private async getGitStatus(ws: WorkspaceInfo): Promise<void> {
    const dir = ws.worktreeDir;
    if (!fs.existsSync(dir)) return;

    const gitExec = (cmd: string): string => {
      try {
        return execSync(cmd, { cwd: dir, stdio: 'pipe', timeout: 5000 }).toString().trim();
      } catch {
        return '';
      }
    };

    const modified = gitExec('git diff --name-only').split('\n').filter(Boolean);
    const staged = gitExec('git diff --cached --name-only').split('\n').filter(Boolean);
    const untracked = gitExec('git ls-files --others --exclude-standard').split('\n').filter(Boolean);
    const aheadStr = gitExec('git rev-list --count @{u}..HEAD 2>/dev/null || echo "0"');
    const commitsAhead = parseInt(aheadStr) || 0;
    const headCommit = gitExec('git log -1 --format=%h');
    const headMessage = gitExec('git log -1 --format=%s');

    ws.git = {
      modified,
      staged,
      untracked,
      commitsAhead,
      headCommit,
      headMessage,
    };
  }
}
