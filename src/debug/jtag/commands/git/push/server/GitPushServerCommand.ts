/**
 * Git Push Command - Server Implementation
 *
 * Push workspace branch to remote repository
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GitPushParams, GitPushResult } from '../shared/GitPushTypes';
import { createGitPushResultFromParams } from '../shared/GitPushTypes';
import * as path from 'path';
import * as fs from 'fs';
import { promisify } from 'util';
import { exec } from 'child_process';

const execAsync = promisify(exec);

export class GitPushServerCommand extends CommandBase<GitPushParams, GitPushResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Push', context, subpath, commander);
  }

  async execute(params: GitPushParams): Promise<GitPushResult> {
    try {
      const userId = params.userId || 'unknown';
      const workspacePath = params.workspacePath || path.resolve(
        process.cwd(), '.continuum/sessions/user/shared', userId, 'workspace'
      );

      if (!fs.existsSync(workspacePath)) {
        throw new Error(`Workspace not found at ${workspacePath}`);
      }

      const remote = params.remote || 'origin';

      // Get current branch
      const { stdout: branchOutput } = await execAsync('git branch --show-current', { cwd: workspacePath });
      const branch = branchOutput.trim();

      // Count commits ahead
      const { stdout: countOutput } = await execAsync(
        `git rev-list --count ${remote}/${branch}..HEAD 2>/dev/null || echo "0"`,
        { cwd: workspacePath }
      );
      const commitsPushed = parseInt(countOutput.trim()) || 0;

      // Push to remote
      await execAsync(`git push -u ${remote} ${branch}`, { cwd: workspacePath });

      console.log(`✅ Pushed ${commitsPushed} commits to ${remote}/${branch}`);

      return createGitPushResultFromParams(params, {
        success: true,
        branch,
        remote,
        commitsPushed
      });

    } catch (error: any) {
      console.error('❌ Git push failed:', error);
      return createGitPushResultFromParams(params, {
        success: false,
        error: error.message || 'Failed to push',
        branch: '',
        remote: '',
        commitsPushed: 0
      });
    }
  }
}
