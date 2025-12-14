/**
 * Git Commit Command - Server Implementation
 *
 * Commit changes in git workspace with persona identity
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { ValidationError } from '../../../../system/core/types/ErrorTypes';
import type { GitCommitParams, GitCommitResult } from '../shared/GitCommitTypes';
import { createGitCommitResultFromParams } from '../shared/GitCommitTypes';
import * as path from 'path';
import * as fs from 'fs';
import { promisify } from 'util';
import { exec } from 'child_process';

const execAsync = promisify(exec);

export class GitCommitServerCommand extends CommandBase<GitCommitParams, GitCommitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Commit', context, subpath, commander);
  }

  async execute(params: GitCommitParams): Promise<GitCommitResult> {
    console.log('🔧 SERVER: Executing Git Commit', params);

    try {
      // 1. Validate message parameter
      if (!params.message || params.message.trim() === '') {
        throw new ValidationError(
          'message',
          'Commit message is required. Use --message="Your commit message".\n' +
          'Use the help tool with "Git Commit" or see the Git Commit README for usage information.'
        );
      }

      // 2. Determine workspace path
      const userId = params.userId || 'unknown';
      const workspacePath = params.workspacePath || path.resolve(
        process.cwd(),
        '.continuum/sessions/user/shared',
        userId,
        'workspace'
      );

      // 3. Verify workspace exists
      if (!fs.existsSync(workspacePath)) {
        throw new Error(
          `Workspace not found at ${workspacePath}. ` +
          'Run git/workspace/init first to create a workspace.'
        );
      }

      // 4. Stage files (specific files or all changes)
      if (params.files && params.files.length > 0) {
        // Stage specific files
        const filesArg = params.files.join(' ');
        await execAsync(`git add ${filesArg}`, { cwd: workspacePath });
      } else {
        // Stage all changes
        await execAsync('git add -A', { cwd: workspacePath });
      }

      // 5. Commit with --no-verify (skip precommit hook for AI commits)
      const { stdout: commitOutput } = await execAsync(
        `git commit --no-verify -m "${params.message.replace(/"/g, '\\"')}"`,
        { cwd: workspacePath }
      );

      // 6. Get commit hash
      const { stdout: commitHash } = await execAsync(
        'git rev-parse HEAD',
        { cwd: workspacePath }
      );
      const fullHash = commitHash.trim();
      const shortHash = fullHash.substring(0, 7);

      // 7. Count files committed
      const { stdout: filesOutput } = await execAsync(
        'git diff-tree --no-commit-id --name-only -r HEAD',
        { cwd: workspacePath }
      );
      const filesCommitted = filesOutput.trim().split('\n').filter(f => f).length;

      console.log(`✅ Committed ${filesCommitted} files: ${shortHash}`);

      return createGitCommitResultFromParams(params, {
        success: true,
        commitHash: fullHash,
        shortHash,
        filesCommitted
      });

    } catch (error: any) {
      console.error('❌ Git commit failed:', error);
      return createGitCommitResultFromParams(params, {
        success: false,
        error: error.message || 'Failed to commit changes',
        commitHash: '',
        shortHash: '',
        filesCommitted: 0
      });
    }
  }
}
