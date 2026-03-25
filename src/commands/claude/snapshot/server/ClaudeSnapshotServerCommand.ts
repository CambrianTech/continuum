/**
 * Claude Snapshot Command - Server Implementation
 *
 * Saves a work-state snapshot for session continuity.
 * Snapshots stored as timestamped JSON in ~/.continuum/claude/snapshots/.
 * Next Claude instance reads latest.json to resume instantly.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { ClaudeSnapshotParams, ClaudeSnapshotResult } from '../shared/ClaudeSnapshotTypes';
import { createClaudeSnapshotResultFromParams } from '../shared/ClaudeSnapshotTypes';
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

export class ClaudeSnapshotServerCommand extends CommandBase<ClaudeSnapshotParams, ClaudeSnapshotResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('claude/snapshot', context, subpath, commander);
  }

  async execute(params: ClaudeSnapshotParams): Promise<ClaudeSnapshotResult> {
    if (!params.summary || params.summary.trim() === '') {
      throw new ValidationError(
        'summary',
        `Missing required parameter 'summary'. What were you working on? This is how the next session knows where to pick up.`
      );
    }

    const timestamp = new Date().toISOString();
    const snapshotId = `snapshot-${timestamp.replace(/[:.]/g, '-')}`;

    // Gather automatic git context
    const repoRoot = process.cwd().replace(/\/src$/, '');
    let gitBranch = 'unknown';
    let gitStatus = '';
    let recentCommits = '';
    try {
      const opts = { cwd: repoRoot, encoding: 'utf-8' as const, timeout: 10000 };
      gitBranch = execSync('git branch --show-current', opts).trim();
      gitStatus = execSync('git status --short', opts).trim();
      recentCommits = execSync('git log --oneline -5', opts).trim();
    } catch { /* git unavailable */ }

    const snapshot = {
      id: snapshotId,
      timestamp,
      summary: params.summary,
      pendingWork: params.pendingWork || null,
      nextSteps: params.nextSteps || null,
      decisions: params.decisions || null,
      issuesWorked: params.issuesWorked ? params.issuesWorked.split(',').map(s => s.trim()) : [],
      git: {
        branch: gitBranch,
        uncommitted: gitStatus || '(clean)',
        recentCommits,
      },
    };

    // Save to ~/.continuum/claude/snapshots/
    const homeDir = process.env.HOME || '/tmp';
    const snapshotDir = path.join(homeDir, '.continuum', 'claude', 'snapshots');
    fs.mkdirSync(snapshotDir, { recursive: true });

    const filePath = path.join(snapshotDir, `${snapshotId}.json`);
    fs.writeFileSync(filePath, JSON.stringify(snapshot, null, 2));

    // Also write latest.json for easy access
    const latestPath = path.join(snapshotDir, 'latest.json');
    fs.writeFileSync(latestPath, JSON.stringify(snapshot, null, 2));

    return createClaudeSnapshotResultFromParams(params, {
      success: true,
      snapshotId,
      filePath,
      timestamp,
    });
  }
}
