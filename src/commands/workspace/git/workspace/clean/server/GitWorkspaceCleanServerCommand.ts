/**
 * Git Workspace Clean Command - Server Implementation
 *
 * Clean up git workspace and remove worktree
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GitWorkspaceCleanParams, GitWorkspaceCleanResult } from '../shared/GitWorkspaceCleanTypes';
import { createGitWorkspaceCleanResultFromParams } from '../shared/GitWorkspaceCleanTypes';
import { resolveWorkspacePathFromUserId } from '../../../shared/resolveWorkspacePath';
import * as path from 'path';
import * as fs from 'fs';
import { promisify } from 'util';
import { exec } from 'child_process';

const execAsync = promisify(exec);

export class GitWorkspaceCleanServerCommand extends CommandBase<GitWorkspaceCleanParams, GitWorkspaceCleanResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Workspace Clean', context, subpath, commander);
  }

  async execute(params: GitWorkspaceCleanParams): Promise<GitWorkspaceCleanResult> {
    try {
      const userId = params.userId || 'unknown';
      const workspacePath = params.workspacePath || await resolveWorkspacePathFromUserId(userId);

      if (!fs.existsSync(workspacePath)) {
        return createGitWorkspaceCleanResultFromParams(params, {
          success: true,
          workspaceRemoved: false,
          branchDeleted: false
        });
      }

      // Get branch name before removing
      const { stdout: branchOutput } = await execAsync('git branch --show-current', { cwd: workspacePath });
      const branch = branchOutput.trim();

      const mainRepoPath = process.cwd();
      const forceFlag = params.force ? '--force' : '';

      // Remove worktree
      await execAsync(`git worktree remove ${forceFlag} "${workspacePath}"`, { cwd: mainRepoPath });
      console.log(`✅ Removed worktree at ${workspacePath}`);

      let branchDeleted = false;
      if (params.deleteBranch && branch) {
        try {
          await execAsync(`git branch -D ${branch}`, { cwd: mainRepoPath });
          branchDeleted = true;
          console.log(`✅ Deleted branch ${branch}`);
        } catch (error) {
          console.warn(`⚠️  Could not delete branch ${branch}`);
        }
      }

      return createGitWorkspaceCleanResultFromParams(params, {
        success: true,
        workspaceRemoved: true,
        branchDeleted
      });

    } catch (error: any) {
      console.error('❌ Workspace cleanup failed:', error);
      return createGitWorkspaceCleanResultFromParams(params, {
        success: false,
        error: error.message || 'Failed to clean workspace',
        workspaceRemoved: false,
        branchDeleted: false
      });
    }
  }
}
