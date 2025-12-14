/**
 * Git Status Command - Server Implementation
 *
 * Show git workspace status and changes
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GitStatusParams, GitStatusResult } from '../shared/GitStatusTypes';
import { createGitStatusResultFromParams } from '../shared/GitStatusTypes';
import * as path from 'path';
import * as fs from 'fs';
import { promisify } from 'util';
import { exec } from 'child_process';

const execAsync = promisify(exec);

export class GitStatusServerCommand extends CommandBase<GitStatusParams, GitStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Status', context, subpath, commander);
  }

  async execute(params: GitStatusParams): Promise<GitStatusResult> {
    try {
      const userId = params.userId || 'unknown';
      const workspacePath = params.workspacePath || path.resolve(
        process.cwd(), '.continuum/sessions/user/shared', userId, 'workspace'
      );

      if (!fs.existsSync(workspacePath)) {
        throw new Error(`Workspace not found at ${workspacePath}`);
      }

      // Get branch name
      const { stdout: branchOutput } = await execAsync('git branch --show-current', { cwd: workspacePath });
      const branch = branchOutput.trim();

      // Get modified files
      const { stdout: modifiedOutput } = await execAsync('git diff --name-only', { cwd: workspacePath });
      const modified = modifiedOutput.trim().split('\n').filter(f => f);

      // Get staged files
      const { stdout: stagedOutput } = await execAsync('git diff --cached --name-only', { cwd: workspacePath });
      const staged = stagedOutput.trim().split('\n').filter(f => f);

      // Get untracked files
      const { stdout: untrackedOutput } = await execAsync('git ls-files --others --exclude-standard', { cwd: workspacePath });
      const untracked = untrackedOutput.trim().split('\n').filter(f => f);

      // Get commits ahead of remote
      const { stdout: aheadOutput } = await execAsync('git rev-list --count @{u}..HEAD 2>/dev/null || echo "0"', { cwd: workspacePath });
      const commitsAhead = parseInt(aheadOutput.trim()) || 0;

      return createGitStatusResultFromParams(params, {
        success: true,
        branch,
        modified,
        staged,
        untracked,
        commitsAhead
      });

    } catch (error: any) {
      return createGitStatusResultFromParams(params, {
        success: false,
        error: error.message || 'Failed to get status',
        branch: '',
        modified: [],
        staged: [],
        untracked: [],
        commitsAhead: 0
      });
    }
  }
}
